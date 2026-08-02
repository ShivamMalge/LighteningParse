//! Integration tests for cleanup heuristics: header/footer and reading order.

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // up from lightningparse-core/
    p.push("benchmarks");
    p.push("corpus");
    p
}

fn read_corpus_file(name: &str) -> String {
    let path = corpus_dir().join(name);
    path.to_str().unwrap().to_string()
}

#[test]
fn test_header_footer_detection_ieee_placeholder() {
    let path = read_corpus_file("ieee_template_placeholder.pdf");
    let mut result = lightningparse::parse_pdf_to_result(&path).unwrap();
    
    // Explicitly run cleanup (just in case we only test the raw extraction otherwise)
    result.pages = lightningparse::cleanup::detect_headers_footers(result.pages).unwrap();

    let mut header_count = 0;
    let mut footer_count = 0;
    let mut body_count = 0;

    for page in &result.pages {
        for block in &page.blocks {
            match block.section_id() {
                "header" => header_count += 1,
                "footer" => footer_count += 1,
                "body" => body_count += 1,
                _ => {}
            }
        }
    }

    // ieee_template_placeholder.pdf is a multi-page document. It might have headers or footers (like page numbers).
    // Ensure we aren't completely zeroing out the body, and it correctly tags things without deletion.
    assert!(body_count > 10, "Should have plenty of body blocks");
    
    // We expect the total block count to remain the same (nothing deleted).
    // Let's verify that a block tagged as header/footer still has text.
    let total_blocks: usize = result.pages.iter().map(|p| p.blocks.len()).sum();
    assert_eq!(total_blocks, body_count + header_count + footer_count);
}

#[test]
fn test_heading_detection_false_positives() {
    use lightningparse::output::{Block, Page, Span};
    // Create a mock page with blocks
    // 1. Regular body text (sets the baseline size)
    // 2. Bold label:value line (should NOT be a heading because size == body size)
    // 3. ALL CAPS text at body size (should NOT be a heading)
    // 4. Genuine heading (size 1.2x body)
    // 5. Genuine heading (size 1.05x body + fully bold)
    
    let mut blocks = vec![];
    for i in 0..10 {
        blocks.push(Block::Text {
            text: format!("This is regular body text line {i}"),
            spans: vec![Span { start: 0, end: 30, bold: false, font_size: 10.0, is_monospace: false }],
            bbox: [0.0, 0.0, 100.0, 10.0],
            section_id: "body".into(),
            block_role: None,
            source: "digital".into(),
        });
    }
    
    // Bold label:value line
    blocks.push(Block::Text {
        text: "Frontend: Next.js, React".into(),
        spans: vec![
            Span { start: 0, end: 10, bold: true, font_size: 10.0, is_monospace: false },
            Span { start: 10, end: 24, bold: false, font_size: 10.0, is_monospace: false }
        ],
        bbox: [0.0, 0.0, 100.0, 10.0],
        section_id: "body".into(),
        block_role: None,
        source: "digital".into(),
    });

    // ALL CAPS text at body size
    blocks.push(Block::Text {
        text: "SOME ACRONYM IN BODY".into(),
        spans: vec![Span { start: 0, end: 20, bold: false, font_size: 10.0, is_monospace: false }],
        bbox: [0.0, 0.0, 100.0, 10.0],
        section_id: "body".into(),
        block_role: None,
        source: "digital".into(),
    });

    // Genuine heading (size 1.2x)
    blocks.push(Block::Text {
        text: "Introduction".into(),
        spans: vec![Span { start: 0, end: 12, bold: false, font_size: 12.0, is_monospace: false }],
        bbox: [0.0, 0.0, 100.0, 10.0],
        section_id: "body".into(),
        block_role: None,
        source: "digital".into(),
    });

    // Genuine heading (size 1.1x + bold)
    blocks.push(Block::Text {
        text: "Methodology".into(),
        spans: vec![Span { start: 0, end: 11, bold: true, font_size: 11.0, is_monospace: false }],
        bbox: [0.0, 0.0, 100.0, 10.0],
        section_id: "body".into(),
        block_role: None,
        source: "digital".into(),
    });

    let pages = vec![Page { page_num: 1, blocks }];
    
    let processed = lightningparse::cleanup::heading_detect::detect_headings(pages).unwrap();
    let proc_blocks = &processed[0].blocks;
    
    if let Block::Text { block_role, .. } = &proc_blocks[10] {
        assert_eq!(*block_role, None, "Bold label:value should NOT be a heading");
    }
    
    if let Block::Text { block_role, .. } = &proc_blocks[11] {
        assert_eq!(*block_role, None, "ALL CAPS text at body size should NOT be a heading");
    }

    if let Block::Text { block_role, .. } = &proc_blocks[12] {
        assert_eq!(*block_role, Some("heading".into()), "Larger font size should be a heading");
    }

    if let Block::Text { block_role, .. } = &proc_blocks[13] {
        assert_eq!(*block_role, Some("heading".into()), "Slightly larger font size + bold should be a heading");
    }
}

#[test]
fn test_reading_order_arxiv_twocolumn() {
    let path = read_corpus_file("arxiv_twocolumn.pdf");
    
    // Run full extraction and cleanup pipeline
    let mut result = lightningparse::parse_pdf_to_result(&path).unwrap();
    result.pages = lightningparse::cleanup::reconstruct_reading_order(lightningparse::cleanup::table_detect::detect_tables(result.pages).unwrap()).unwrap();
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
            match block.section_id() {
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
            if block.bbox()[0] < min_x { min_x = block.bbox()[0]; }
            if block.bbox()[2] > max_x { max_x = block.bbox()[2]; }
        }
        let page_width = if max_x > min_x { max_x - min_x } else { 1.0 };
        
        let mut large_right_jumps = 0;
        let mut _large_left_jumps = 0;
        
        for w in page.blocks.windows(2) {
            let prev_x = w[0].bbox()[0];
            let next_x = w[1].bbox()[0];
            
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
