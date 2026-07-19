//! Header/footer detection and reading-order reconstruction.
//!
//! Cross-page heuristic: buckets blocks by normalized y-position,
//! clusters by text similarity across pages, and tags (not deletes)
//! blocks appearing in margin bands on ≥N% of pages.

use crate::errors::ParseError;
use crate::output::Page;

/// Detect and tag headers/footers across all pages.
///
/// Modifies `section_id` on blocks in-place. Does NOT remove blocks —
/// downstream consumers decide whether to filter headers/footers.
///
/// Phase 0 stub — no-op, returns pages unchanged.
pub fn detect_headers_footers(pages: Vec<Page>) -> Result<Vec<Page>, ParseError> {
    // No-op stub: return pages unmodified
    Ok(pages)
}
