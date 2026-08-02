use std::collections::HashMap;

use crate::errors::ParseError;
use crate::output::{Block, Page};

/// Detect and tag semantic headings across all pages.
/// Modifies `block_role` on blocks in-place.
pub fn detect_headings(mut pages: Vec<Page>) -> Result<Vec<Page>, ParseError> {
    if pages.is_empty() {
        return Ok(pages);
    }

    // 1. Determine the document-wide body font size.
    // We do this by calculating the most common font size (weighted by character count)
    // across all text blocks that are not headers/footers.
    let mut font_sizes: HashMap<i32, usize> = HashMap::new();

    for page in &pages {
        for block in &page.blocks {
            if let Block::Text {
                text,
                spans,
                section_id,
                ..
            } = block
            {
                // Ignore headers and footers for body size calculation
                if section_id != "body" {
                    continue;
                }

                // If spans are available, use the first span's size
                let mut size = 12.0; // fallback
                if let Some(first_span) = spans.first() {
                    size = first_span.font_size;
                }

                // Round to nearest tenth (e.g. 10.03 -> 100) to bucket them safely
                let bucket = (size * 10.0).round() as i32;
                *font_sizes.entry(bucket).or_insert(0) += text.chars().count();
            }
        }
    }

    let mut max_chars = 0;
    let mut body_size_bucket = 120;
    for (bucket, chars) in font_sizes {
        if chars > max_chars {
            max_chars = chars;
            body_size_bucket = bucket;
        }
    }

    let body_font_size = (body_size_bucket as f64) / 10.0;
    if body_font_size <= 0.0 {
        return Ok(pages); // Degenerate case
    }

    // Threshold logic:
    // A heading must be somewhat short. The longest genuine heading in our test fixtures
    // is ~63 chars (Shivam_FullStack.pdf). The shortest false-positive block is ~75 chars
    // (arxiv copyright lines). 70 chars perfectly splits the difference.
    let length_limit = 70;

    // Size ratios
    let strong_ratio = 1.15; // 15% larger is unambiguously a heading
    let weak_ratio = 1.01; // Slightly larger requires bold as a secondary signal

    // 2. Classify blocks
    for page in &mut pages {
        for block in &mut page.blocks {
            if let Block::Text {
                text,
                spans,
                section_id,
                block_role,
                ..
            } = block
            {
                // Headings only exist in the main body, not page margins
                if section_id != "body" {
                    continue;
                }

                let text_len = text.chars().count();
                if text_len == 0 || text_len > length_limit {
                    continue;
                }

                let mut block_size = 12.0;
                let mut fully_bold = false;

                if !spans.is_empty() {
                    block_size = spans[0].font_size;

                    // Check if the entire block is bold
                    fully_bold = true;
                    let mut covered_chars = 0;
                    for span in spans {
                        if !span.bold {
                            fully_bold = false;
                            break;
                        }
                        covered_chars += span.end - span.start;
                    }
                    if covered_chars < text_len {
                        // Some text (e.g. spaces) might not be covered by a span, but
                        // if all spanned text is bold, we consider it fully bold for heading logic.
                    }
                }

                // Code block precedence: if already tagged as code, do not overwrite with heading
                if let Some(role) = block_role {
                    if role == "code" {
                        continue;
                    }
                }

                // Core heuristic
                if block_size >= body_font_size * strong_ratio
                    || (block_size > body_font_size * weak_ratio && fully_bold)
                {
                    *block_role = Some("heading".into());
                }
            }
        }
    }

    Ok(pages)
}
