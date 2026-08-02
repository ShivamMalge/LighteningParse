use crate::errors::ParseError;
use crate::output::{Block, Page};

struct CandidateRow {
    blocks: Vec<Block>,
}

pub fn detect_tables(mut pages: Vec<Page>) -> Result<Vec<Page>, ParseError> {
    for page in &mut pages {
        if page.blocks.is_empty() {
            continue;
        }

        // 1. Estimate font size for tolerance
        // Use median height of text blocks as a rough font size proxy
        let mut heights: Vec<f64> = page
            .blocks
            .iter()
            .map(|b| {
                let bbox = b.bbox();
                (bbox[3] - bbox[1]).abs()
            })
            .collect();
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let font_size = if heights.is_empty() {
            12.0
        } else {
            heights[heights.len() / 2]
        };

        let y_tolerance = font_size * 0.5;

        // 2. Group blocks into CandidateRows
        let mut blocks = std::mem::take(&mut page.blocks);
        // Sort blocks by max_y descending (top to bottom)
        blocks.sort_by(|a, b| b.bbox()[3].partial_cmp(&a.bbox()[3]).unwrap());

        let mut candidate_rows: Vec<CandidateRow> = Vec::new();
        let mut current_row: Vec<Block> = Vec::new();
        let mut current_max_y: Option<f64> = None;

        for block in blocks {
            let by = block.bbox()[3];
            if let Some(my) = current_max_y {
                if (by - my).abs() <= y_tolerance {
                    current_row.push(block);
                } else {
                    candidate_rows.push(CandidateRow {
                        blocks: current_row,
                    });
                    current_row = vec![block];
                    current_max_y = Some(by);
                }
            } else {
                current_row.push(block);
                current_max_y = Some(by);
            }
        }
        if !current_row.is_empty() {
            candidate_rows.push(CandidateRow {
                blocks: current_row,
            });
        }

        // 3. Filter rows (tables need columns, so > 1 block)
        // Actually, we shouldn't filter them out completely because we need them back in the page if they aren't part of a table.
        // Instead, we will iterate and identify contiguous runs of "table rows".

        let mut final_blocks: Vec<Block> = Vec::new();

        let mut i = 0;
        while i < candidate_rows.len() {
            let mut table_run = 1;

            // Look ahead for matching columns
            while i + table_run < candidate_rows.len() {
                let r1 = &candidate_rows[i + table_run - 1];
                let r2 = &candidate_rows[i + table_run];

                // Are these rows structurally similar?
                // Both must have > 1 block
                if r1.blocks.len() <= 1 || r2.blocks.len() <= 1 {
                    break;
                }

                // Sort blocks left-to-right
                let mut r1_sorted = r1.blocks.clone();
                r1_sorted.sort_by(|a, b| a.bbox()[0].partial_cmp(&b.bbox()[0]).unwrap());

                let mut r2_sorted = r2.blocks.clone();
                r2_sorted.sort_by(|a, b| a.bbox()[0].partial_cmp(&b.bbox()[0]).unwrap());

                // Count matching column boundaries (min_x)
                let mut matches = 0;

                for b1 in &r1_sorted {
                    for b2 in &r2_sorted {
                        // Check if blocks overlap horizontally
                        let overlap =
                            b1.bbox()[2].min(b2.bbox()[2]) - b1.bbox()[0].max(b2.bbox()[0]);
                        if overlap > 0.0 {
                            matches += 1;
                            break;
                        }
                    }
                }

                // If at least 2 columns align, we consider them part of the same table
                if matches >= 2 {
                    table_run += 1;
                } else {
                    break;
                }
            }

            // Check if we found a valid table (>= 3 rows)
            let mut total_chars = 0;
            let mut total_blocks = 0;
            for r in 0..table_run {
                for b in &candidate_rows[i + r].blocks {
                    total_chars += b.text().trim().len();
                    total_blocks += 1;
                }
            }
            let avg_chars = if total_blocks > 0 {
                total_chars as f64 / total_blocks as f64
            } else {
                0.0
            };

            // Check if there is a "Table " or "TABLE " caption nearby (look up to 6 rows above and below)
            let mut has_caption = false;
            let look_range = 6;

            // Check preceding rows
            for j in 1..=look_range {
                if i >= j {
                    let text = candidate_rows[i - j]
                        .blocks
                        .iter()
                        .map(|b| b.text().trim())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase();
                    if text.contains("table ") || text.starts_with("table") {
                        has_caption = true;
                        break;
                    }
                }
            }

            // Check succeeding rows
            if !has_caption {
                for j in 0..look_range {
                    if i + table_run + j < candidate_rows.len() {
                        let text = candidate_rows[i + table_run + j]
                            .blocks
                            .iter()
                            .map(|b| b.text().trim())
                            .collect::<Vec<_>>()
                            .join(" ")
                            .to_lowercase();
                        if text.contains("table ") || text.starts_with("table") {
                            has_caption = true;
                            break;
                        }
                    }
                }
            }

            // Heuristic: tables usually have short cell content. If the average block length
            // is very high, it's likely a multi-column text layout falsely matching the geometry.
            // Require a caption context to cleanly distinguish from author blocks and math equations.
            let is_table_shape = has_caption && table_run >= 2;
            if is_table_shape && avg_chars < 30.0 {
                // Construct Table Block
                let mut min_x = f64::MAX;
                let mut min_y = f64::MAX;
                let mut max_x = f64::MIN;
                let mut max_y = f64::MIN;

                let mut table_rows: Vec<Vec<String>> = Vec::new();

                for r in 0..table_run {
                    let mut row_blocks = candidate_rows[i + r].blocks.clone();
                    // sort left-to-right
                    row_blocks.sort_by(|a, b| a.bbox()[0].partial_cmp(&b.bbox()[0]).unwrap());

                    let mut text_row = Vec::new();
                    for b in row_blocks {
                        min_x = min_x.min(b.bbox()[0]);
                        min_y = min_y.min(b.bbox()[1]);
                        max_x = max_x.max(b.bbox()[2]);
                        max_y = max_y.max(b.bbox()[3]);
                        text_row.push(b.text().trim().to_string());
                    }
                    table_rows.push(text_row);
                }

                let source = candidate_rows[i]
                    .blocks
                    .first()
                    .unwrap()
                    .source()
                    .to_string();

                final_blocks.push(Block::Table {
                    rows: table_rows,
                    bbox: [min_x, min_y, max_x, max_y],
                    section_id: "body".to_string(), // Reading order or header detection can modify this later
                    block_role: None,
                    source,
                });

                i += table_run;
            } else {
                // Not a table, just add the individual blocks from the entire run
                // Skip the whole run to avoid O(N^2) backtracking
                for r in 0..table_run {
                    final_blocks.append(&mut candidate_rows[i + r].blocks);
                }
                i += table_run;
            }
        }

        page.blocks = final_blocks;
    }

    Ok(pages)
}
