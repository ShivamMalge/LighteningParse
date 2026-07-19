//! Tier 2: Scanned-page detection and OCR fallback.
//!
//! Detects pages without an extractable text layer and routes
//! them through an OCR engine (Tesseract in v1).

use crate::errors::ParseError;
use crate::output::Page;

/// Run OCR on a single scanned page image.
///
/// Phase 0 stub — returns an error indicating not yet implemented.
pub fn ocr_page(_page_image: &[u8], _page_num: u32) -> Result<Page, ParseError> {
    Err(ParseError::OcrEngine("OCR not yet implemented".into()))
}
