//! Shared error types for the LightningParse core.
//!
//! Lives outside `ffi/` so extraction, cleanup, and OCR modules
//! can return typed errors without depending on PyO3.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Corrupt PDF: {0}")]
    CorruptPdf(String),

    #[error("Unsupported PDF feature: {0}")]
    UnsupportedPdf(String),

    #[error("OCR engine error: {0}")]
    OcrEngine(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
