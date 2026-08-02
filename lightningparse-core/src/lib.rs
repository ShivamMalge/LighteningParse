use crate::errors::ParseError;
use crate::output::{ParseResult, DocumentMetadata, Page};
use lopdf::Document;

pub mod cleanup;
pub mod errors;
pub mod extract;
pub mod ocr;
pub mod output;
pub mod ffi;

pub fn parse_pdf_to_result(path: &str) -> Result<ParseResult, ParseError> {
    let pdf_bytes = std::fs::read(path).map_err(ParseError::Io)?;
    let start = std::time::Instant::now();
    
    let doc = Document::load_mem(&pdf_bytes)
        .map_err(|e| ParseError::CorruptPdf(format!("Failed to parse PDF: {e}")))?;

    let extract_results = extract::extract_text(&doc)?;
    
    let mut pages = Vec::new();
    let mut total_digital_pages = 0;
    let mut total_scanned_pages = 0;
    let mut all_warnings = Vec::new();

    for (page_num, mut blocks, total_chars, mut warnings) in extract_results {
        all_warnings.append(&mut warnings);
        if total_chars == 0 {
            // OCR fallback
            let ocr_blocks = ocr::extract_page_ocr(path, page_num)?;
            
            // Map ocr::extract::RawBlock to output::Block is done in cleanup step later? 
            // Wait, RawBlock is what extract_text returns.
            blocks = ocr_blocks;
            total_scanned_pages += 1;
        } else {
            total_digital_pages += 1;
        }
        
        pages.push(Page {
            page_num,
            blocks,
        });
    }

    let tier = if total_digital_pages > 0 && total_scanned_pages > 0 {
        "mixed"
    } else if total_scanned_pages > 0 {
        "scanned"
    } else {
        "digital"
    };

    let parse_time_ms = start.elapsed().as_millis() as u64;
    let page_count = pages.len() as u32;

    Ok(ParseResult {
        pages,
        metadata: DocumentMetadata {
            tier: tier.to_string(),
            page_count,
            parse_time_ms,
            warnings: all_warnings,
        },
    })
}
