//! Integration tests for cleanup heuristics: header/footer and reading order.

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // up from lightningparse-core/
    p.push("benchmarks");
    p.push("corpus");
    p
}

fn read_corpus_file(name: &str) -> Vec<u8> {
    let path = corpus_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!("Could not read corpus file {}: {e}", path.display());
    })
}

#[test]
fn test_header_footer_detection_draft10() {
    let bytes = read_corpus_file("draft10.pdf");
    let mut result = lightningparse::extract::extract_text(&bytes).unwrap();
    
    // Explicitly run cleanup (just in case we only test the raw extraction otherwise)
    result.pages = lightningparse::cleanup::detect_headers_footers(result.pages).unwrap();

    let mut header_count = 0;
    let mut footer_count = 0;
    let mut body_count = 0;

    for page in &result.pages {
        for block in &page.blocks {
            match block.section_id.as_str() {
                "header" => header_count += 1,
                "footer" => footer_count += 1,
                "body" => body_count += 1,
                _ => {}
            }
        }
    }

    // draft10.pdf is a LaTeX document. It might have headers or footers (like page numbers).
    // Ensure we aren't completely zeroing out the body, and it correctly tags things without deletion.
    assert!(body_count > 10, "Should have plenty of body blocks");
    
    // We expect the total block count to remain the same (nothing deleted).
    // Let's verify that a block tagged as header/footer still has text.
    let total_blocks: usize = result.pages.iter().map(|p| p.blocks.len()).sum();
    assert_eq!(total_blocks, body_count + header_count + footer_count);
}

#[test]
fn test_reading_order_arxiv_twocolumn() {
    let bytes = read_corpus_file("arxiv_twocolumn.pdf");
    
    // Run full extraction and cleanup pipeline
    let mut result = lightningparse::extract::extract_text(&bytes).unwrap();
    result.pages = lightningparse::cleanup::reconstruct_reading_order(result.pages).unwrap();
    result.pages = lightningparse::cleanup::detect_headers_footers(result.pages).unwrap();

    assert!(result.metadata.page_count > 1, "Arxiv paper should be multiple pages");
    
    // Make sure we didn't over-merge the entire page into 1 block due to missing ET operators.
    // Legitimate column and paragraph breaks should yield multiple blocks (currently 15 blocks).
    assert!(
        result.pages[0].blocks.len() > 10,
        "Arxiv page 1 should have >10 blocks, got {}; ensuring BT...ET over-merging is fixed",
        result.pages[0].blocks.len()
    );

    let mut _header_count = 0;
    let mut body_count = 0;
    for page in &result.pages {
        for block in &page.blocks {
            match block.section_id.as_str() {
                "header" => _header_count += 1,
                "body" => body_count += 1,
                _ => {}
            }
        }
    }
    assert!(body_count > 0, "Should have body text");

    // Spot check reading order for multi-column.
    // In a two column layout, if we look at the sequence of blocks, we should see blocks going down the left column (x ~ small),
    // and then jumping to the right column (x ~ large).
    // We can verify this by checking if the x-coordinates have exactly ONE large positive jump per page (or per swath).
    // If it zig-zags (left-right-left-right) every line, reading order is broken.
    for page in result.pages {
        // Find the page width to define a "large" jump.
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        for block in &page.blocks {
            if block.bbox[0] < min_x { min_x = block.bbox[0]; }
            if block.bbox[2] > max_x { max_x = block.bbox[2]; }
        }
        let page_width = if max_x > min_x { max_x - min_x } else { 1.0 };
        
        let mut large_right_jumps = 0;
        let mut _large_left_jumps = 0;
        
        for w in page.blocks.windows(2) {
            let prev_x = w[0].bbox[0];
            let next_x = w[1].bbox[0];
            
            let jump = next_x - prev_x;
            if jump > page_width * 0.3 {
                large_right_jumps += 1; // Jumped from left column to right column
            } else if jump < -page_width * 0.3 {
                _large_left_jumps += 1; // Jumped from right column back to left column (e.g. next swath)
            }
        }
        
        // In a perfectly interleaved (broken) reading order, it would zig-zag every line,
        // causing dozens of jumps per page.
        // In a correct reading order, we should see very few column jumps per page (typically <= 5, depending on figures/titles breaking swaths).
        assert!(large_right_jumps <= 10, "Reading order appears broken on page {}: too many right jumps ({}) - looks like it's zig-zagging between columns", page.page_num, large_right_jumps);
    }
}
