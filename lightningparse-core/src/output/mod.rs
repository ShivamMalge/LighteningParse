//! Structured output types matching the JSON schema in ARCHITECTURE.md §3.1.

use serde::{Deserialize, Serialize};

/// Top-level parse result for an entire document.
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseResult {
    pub pages: Vec<Page>,
    pub metadata: DocumentMetadata,
}

/// Per-page result.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Page {
    pub page_num: u32,
    pub blocks: Vec<Block>,
}

/// A single text block extracted from a page.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block {
    /// Extracted text content.
    pub text: String,
    /// Bounding box: [x0, y0, x1, y1].
    pub bbox: [f64; 4],
    /// Section classification: "header", "body", or "footer".
    pub section_id: String,
    /// Extraction source: "digital" (Tier 1) or "ocr" (Tier 2).
    pub source: String,
}

/// Document-level metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// "digital", "scanned", or "mixed".
    pub tier: String,
    pub page_count: u32,
    pub parse_time_ms: u64,
}
