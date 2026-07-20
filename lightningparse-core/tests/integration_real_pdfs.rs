//! Integration tests using real-world PDF files.
//!
//! These tests exercise the extraction engine on actual documents rather
//! than programmatically generated fixtures.  They verify:
//!   - Parsing succeeds without errors
//!   - Pages and blocks are non-empty (for digital-native PDFs)
//!   - Page order is deterministic (important now that rayon is in play)
//!   - JSON serialisation round-trips cleanly

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

// ── Shivam_FullStack.pdf (image-based / scanned, Tier 2 — not in Tier 1 corpus) ──

#[test]
fn test_real_scanned_pdf_parses_without_error() {
    // Lives in tests/fixtures/tier2/, NOT benchmarks/corpus/ (Tier 1).
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push("tier2");
    path.push("Shivam_FullStack.pdf");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!("Could not read fixture {}: {e}", path.display());
    });

    let result = lightningparse::extract::extract_text(&bytes)
        .expect("Shivam_FullStack.pdf should parse without error");

    assert_eq!(result.metadata.page_count, 1);
    assert_eq!(result.pages.len(), 1);
    assert_eq!(result.metadata.tier, "digital");

    // This PDF is image-based — Tier 1 extraction correctly produces 0 blocks.
    // It will need OCR (Tier 2) in Phase 5.
    let total_blocks: usize = result.pages.iter().map(|p| p.blocks.len()).sum();
    assert_eq!(
        total_blocks, 0,
        "scanned PDF should produce 0 blocks from digital extraction"
    );
}

// ── draft10.pdf (LaTeX-compiled, multi-page, digital-native) ────

#[test]
fn test_real_latex_pdf_parses() {
    let bytes = read_corpus_file("draft10.pdf");
    let result = lightningparse::extract::extract_text(&bytes)
        .expect("draft10.pdf should parse successfully");

    assert!(
        result.metadata.page_count >= 2,
        "LaTeX doc should have multiple pages, got {}",
        result.metadata.page_count,
    );
    assert_eq!(result.pages.len(), result.metadata.page_count as usize);
    assert_eq!(result.metadata.tier, "digital");
}

#[test]
fn test_real_latex_pdf_page_order() {
    let bytes = read_corpus_file("draft10.pdf");
    let result = lightningparse::extract::extract_text(&bytes).unwrap();

    for w in result.pages.windows(2) {
        assert!(
            w[0].page_num < w[1].page_num,
            "pages out of order: {} >= {}",
            w[0].page_num,
            w[1].page_num,
        );
    }
}

#[test]
fn test_real_latex_pdf_has_text() {
    let bytes = read_corpus_file("draft10.pdf");
    let result = lightningparse::extract::extract_text(&bytes).unwrap();

    let total_blocks: usize = result.pages.iter().map(|p| p.blocks.len()).sum();
    assert!(
        total_blocks > 0,
        "LaTeX PDF should produce text blocks"
    );

    // Spot-check: combined text should be substantial.
    let all_text: String = result
        .pages
        .iter()
        .flat_map(|p| p.blocks.iter())
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        all_text.len() > 50,
        "combined text should be substantial, got {} chars",
        all_text.len(),
    );
}

#[test]
fn test_real_latex_pdf_json_roundtrip() {
    let bytes = read_corpus_file("draft10.pdf");
    let result = lightningparse::extract::extract_text(&bytes).unwrap();

    let json = serde_json::to_string(&result).expect("serialisation should work");
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(val["pages"].is_array());
    let pages_arr = val["pages"].as_array().unwrap();
    assert!(pages_arr.len() >= 2);

    // Every block must have the required fields.
    for page in pages_arr {
        for blk in page["blocks"].as_array().unwrap_or(&vec![]) {
            assert!(blk["text"].is_string());
            assert!(blk["bbox"].is_array());
            assert_eq!(blk["bbox"].as_array().unwrap().len(), 4);
            assert!(blk["section_id"].is_string());
            assert!(blk["source"].is_string());
        }
    }
}

// ── Determinism: multiple runs must produce identical content ────

#[test]
fn test_parallel_determinism() {
    let bytes = read_corpus_file("draft10.pdf");

    // Run extraction 5 times; compare page/block content (not timing metadata).
    let first = lightningparse::extract::extract_text(&bytes).unwrap();

    for i in 1..5 {
        let r = lightningparse::extract::extract_text(&bytes).unwrap();

        assert_eq!(
            first.pages.len(),
            r.pages.len(),
            "run {i}: page count differs",
        );

        for (p1, p2) in first.pages.iter().zip(r.pages.iter()) {
            assert_eq!(
                p1.page_num, p2.page_num,
                "run {i}: page_num mismatch",
            );
            assert_eq!(
                p1.blocks.len(),
                p2.blocks.len(),
                "run {i}: block count differs on page {}",
                p1.page_num,
            );
            for (b1, b2) in p1.blocks.iter().zip(p2.blocks.iter()) {
                assert_eq!(
                    b1.text, b2.text,
                    "run {i}: text differs on page {}",
                    p1.page_num,
                );
                assert_eq!(
                    b1.bbox, b2.bbox,
                    "run {i}: bbox differs on page {}",
                    p1.page_num,
                );
            }
        }
    }
}
