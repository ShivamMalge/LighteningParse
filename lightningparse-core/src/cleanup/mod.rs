//! Header/footer detection and reading-order reconstruction.
//!
//! Cross-page heuristic: buckets blocks by normalized y-position,
//! clusters by text similarity across pages, and tags (not deletes)
//! blocks appearing in margin bands on ≥N% of pages.
//!
//! Reading order reconstruction: groups blocks into horizontal swaths
//! separated by full-width blocks, then clusters blocks into columns
//! within each swath based on x-span overlap, and sorts columns left-to-right.

use std::collections::{HashMap, HashSet};

use crate::errors::ParseError;
use crate::output::{Block, Page};

/// Detect and tag headers/footers across all pages.
/// Modifies `section_id` on blocks in-place. Does NOT remove blocks.
pub fn detect_headers_footers(mut pages: Vec<Page>) -> Result<Vec<Page>, ParseError> {
    if pages.is_empty() {
        return Ok(pages);
    }

    let total_pages = pages.len();
    let threshold = (total_pages as f64 * 0.7).ceil() as usize; // 70% of pages

    // 1. Find global max_y across all pages to define bands.
    // (Assuming standard PDF coordinates where y=0 is bottom, so max_y is top of page)
    let mut global_max_y = 0.0;
    for page in &pages {
        for block in &page.blocks {
            if block.bbox[3] > global_max_y {
                global_max_y = block.bbox[3];
            }
        }
    }

    if global_max_y == 0.0 {
        return Ok(pages); // No blocks or degenerate bboxes
    }

    // Define top and bottom margin bands (e.g., 10% of page height).
    // In PDF coords (y=0 at bottom):
    // Top band: y > max_y * 0.90
    // Bottom band: y < max_y * 0.10
    let top_band_threshold = global_max_y * 0.90;
    let bottom_band_threshold = global_max_y * 0.10;

    // 2. Collect block texts by band and page.
    // Key: Normalized text (digits removed, lowercase).
    // Value: Set of page numbers where this text appears in the band.
    let mut top_band_clusters: HashMap<String, HashSet<u32>> = HashMap::new();
    let mut bottom_band_clusters: HashMap<String, HashSet<u32>> = HashMap::new();

    for page in &pages {
        for block in &page.blocks {
            // Check top band
            if block.bbox[1] > top_band_threshold {
                let norm_text = normalize_text(&block.text);
                if !norm_text.is_empty() {
                    top_band_clusters
                        .entry(norm_text)
                        .or_default()
                        .insert(page.page_num);
                }
            }
            // Check bottom band
            else if block.bbox[3] < bottom_band_threshold {
                let norm_text = normalize_text(&block.text);
                if !norm_text.is_empty() {
                    bottom_band_clusters
                        .entry(norm_text)
                        .or_default()
                        .insert(page.page_num);
                }
            }
        }
    }

    // 3. Identify header/footer texts that meet the threshold.
    let header_texts: HashSet<String> = top_band_clusters
        .into_iter()
        .filter(|(_, pages)| pages.len() >= threshold)
        .map(|(text, _)| text)
        .collect();

    let footer_texts: HashSet<String> = bottom_band_clusters
        .into_iter()
        .filter(|(_, pages)| pages.len() >= threshold)
        .map(|(text, _)| text)
        .collect();

    // 4. Tag the blocks.
    for page in &mut pages {
        // We need max_y of the page for single-page heuristics.
        let mut page_max_y = 0.0;
        for block in &page.blocks {
            if block.bbox[3] > page_max_y {
                page_max_y = block.bbox[3];
            }
        }
        let page_top_10 = page_max_y * 0.90;
        let page_bottom_10 = page_max_y * 0.10;
        let page_bottom_30 = page_max_y * 0.30;

        for block in &mut page.blocks {
            let norm_text = normalize_text(&block.text);
            if norm_text.is_empty() {
                continue;
            }

            // Cross-page matches
            if block.bbox[1] > top_band_threshold && header_texts.contains(&norm_text) {
                block.section_id = "header".into();
                continue;
            } else if block.bbox[3] < bottom_band_threshold && footer_texts.contains(&norm_text) {
                block.section_id = "footer".into();
                continue;
            }

            // Single-page fallbacks (only apply to the first page to avoid mistagging top-of-page figures/tables on subsequent pages)
            if page.page_num == 1 {
                // Check footnote first (up to bottom 30%)
                if block.bbox[1] < page_bottom_30 && (block.text.starts_with('*') 
                    || block.text.starts_with('\u{2217}') 
                    || block.text.starts_with('†') 
                    || block.text.starts_with('‡') 
                    || block.text.starts_with('§')) 
                {
                    block.section_id = "footnote".into();
                    continue;
                }

                // Very top blocks lacking cross-page match
                if block.bbox[1] > page_top_10 {
                    block.section_id = "header".into();
                    continue;
                }

                // Very bottom blocks lacking cross-page match
                if block.bbox[3] < page_bottom_10 {
                    block.section_id = "footer".into();
                    continue;
                }
            }
        }
    }

    Ok(pages)
}

/// Normalizes text for header/footer clustering by removing digits and lowercasing.
/// This groups "Page 1" and "Page 2" into the same cluster "page ".
fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase()
        .trim()
        .to_string()
}

/// Reconstructs the reading order of blocks on each page.
///
/// Handles multi-column layouts by grouping blocks into horizontal swaths
/// separated by full-width blocks, clustering blocks into columns within each swath,
/// and sorting columns left-to-right and blocks top-to-bottom.
pub fn reconstruct_reading_order(mut pages: Vec<Page>) -> Result<Vec<Page>, ParseError> {
    for page in &mut pages {
        if page.blocks.is_empty() {
            continue;
        }

        // 1. Calculate page width to identify full-width blocks.
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        for block in &page.blocks {
            if block.bbox[0] < min_x {
                min_x = block.bbox[0];
            }
            if block.bbox[2] > max_x {
                max_x = block.bbox[2];
            }
        }
        let page_width = if max_x > min_x { max_x - min_x } else { 1.0 };
        let full_width_threshold = page_width * 0.65; // Block spanning >65% of page width is full-width.

        // 2. Sort blocks top-to-bottom initially (PDF y=0 is bottom, so max_y descending).
        // Use bbox[3] (max_y) as the primary vertical coordinate.
        page.blocks.sort_by(|a, b| {
            b.bbox[3]
                .partial_cmp(&a.bbox[3])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 3. Group blocks into horizontal swaths.
        let mut swaths: Vec<Vec<Block>> = Vec::new();
        let mut current_swath: Vec<Block> = Vec::new();

        for i in 0..page.blocks.len() {
            let block = page.blocks[i].clone();
            let block_width = block.bbox[2] - block.bbox[0];
            let is_full_width = block_width >= full_width_threshold;

            // Check if this block has any vertical overlap with ANY OTHER block on the page.
            let mut has_horizontal_neighbors = false;
            for j in 0..page.blocks.len() {
                if i == j {
                    continue;
                }
                let other = &page.blocks[j];
                // Y-overlap check
                if block.bbox[1].max(other.bbox[1]) <= block.bbox[3].min(other.bbox[3]) {
                    has_horizontal_neighbors = true;
                    break;
                }
            }

            // A block breaks the swath if it's explicitly full-width, OR if it's the only block in its Y-band (isolated).
            let acts_as_boundary = is_full_width || !has_horizontal_neighbors;

            if acts_as_boundary {
                // Close current swath if it has blocks.
                if !current_swath.is_empty() {
                    swaths.push(current_swath);
                    current_swath = Vec::new();
                }
                // Boundary blocks get their own swath.
                swaths.push(vec![block]);
            } else {
                current_swath.push(block);
            }
        }
        if !current_swath.is_empty() {
            swaths.push(current_swath);
        }
        
        // Clear page.blocks since we cloned them out (to avoid borrow checker issues with drain + iter).
        page.blocks.clear();

        // 4. Within each swath, cluster blocks into columns and sort.
        let mut ordered_blocks = Vec::with_capacity(page.blocks.capacity());

        for swath in swaths {
            if swath.len() <= 1 {
                ordered_blocks.extend(swath);
                continue;
            }

            // Cluster blocks into columns based on x-span overlap.
            // Using a simple Union-Find / Disjoint Set approach.
            let n = swath.len();
            let mut parent: Vec<usize> = (0..n).collect();

            fn find(i: usize, parent: &mut [usize]) -> usize {
                if parent[i] == i {
                    i
                } else {
                    let p = find(parent[i], parent);
                    parent[i] = p;
                    p
                }
            }

            fn union(i: usize, j: usize, parent: &mut [usize]) {
                let root_i = find(i, parent);
                let root_j = find(j, parent);
                if root_i != root_j {
                    parent[root_i] = root_j;
                }
            }

            for i in 0..n {
                for j in (i + 1)..n {
                    let a = &swath[i].bbox;
                    let b = &swath[j].bbox;
                    // Check for x-span overlap.
                    // a overlaps b if max(a.min_x, b.min_x) <= min(a.max_x, b.max_x)
                    let overlap_x = a[0].max(b[0]) <= a[2].min(b[2]);
                    if overlap_x {
                        union(i, j, &mut parent);
                    }
                }
            }

            // Group blocks by their root parent (column).
            let mut columns: HashMap<usize, Vec<Block>> = HashMap::new();
            for (i, block) in swath.into_iter().enumerate() {
                let root = find(i, &mut parent);
                columns.entry(root).or_default().push(block);
            }

            // Convert columns map to a list and compute average min_x for left-to-right sorting.
            let mut col_list: Vec<(f64, Vec<Block>)> = columns
                .into_values()
                .map(|mut col_blocks| {
                    let avg_x = col_blocks.iter().map(|b| b.bbox[0]).sum::<f64>() / (col_blocks.len() as f64);
                    // Sort blocks within column top-to-bottom.
                    col_blocks.sort_by(|a, b| {
                        b.bbox[3]
                            .partial_cmp(&a.bbox[3])
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                a.bbox[0]
                                    .partial_cmp(&b.bbox[0])
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                    });
                    (avg_x, col_blocks)
                })
                .collect();

            // Sort columns left-to-right.
            col_list.sort_by(|(x1, _), (x2, _)| x1.partial_cmp(x2).unwrap_or(std::cmp::Ordering::Equal));

            // Append blocks in reading order.
            for (_, col_blocks) in col_list {
                ordered_blocks.extend(col_blocks);
            }
        }

        page.blocks = ordered_blocks;
    }

    Ok(pages)
}
