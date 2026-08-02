use crate::errors::ParseError;
use crate::output::Block;
use rusty_tesseract::{Args, Image};
use std::process::Command;

pub fn extract_page_ocr(pdf_path: &str, page_num: u32) -> Result<Vec<Block>, ParseError> {
    // We shell out to pdftoppm to render exactly the target page to a temp file
    let temp_dir = tempfile::tempdir()
        .map_err(|e| ParseError::OcrFailed(format!("Failed to create tempdir: {}", e)))?;
    let output_prefix = temp_dir.path().join("page");

    // Execute pdftoppm
    let status = Command::new("pdftoppm")
        .arg("-png")
        .arg("-f")
        .arg(page_num.to_string())
        .arg("-l")
        .arg(page_num.to_string())
        .arg("-r")
        .arg("300") // 300 DPI for good OCR quality
        .arg(pdf_path)
        .arg(output_prefix.to_string_lossy().as_ref())
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return Err(ParseError::OcrFailed(format!(
                "pdftoppm failed with status: {}",
                s
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ParseError::OcrMissingDependency("pdftoppm".to_string()));
        }
        Err(e) => {
            return Err(ParseError::OcrFailed(format!(
                "Failed to execute pdftoppm: {}",
                e
            )))
        }
    }

    // pdftoppm produces files like page-1.png or page-001.png or page-01.png
    // We'll just read the directory to find the generated file.
    let mut image_path = None;
    for entry in std::fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().unwrap_or_default() == "png" {
            image_path = Some(entry.path());
            break;
        }
    }

    let image_path = image_path
        .ok_or_else(|| ParseError::OcrFailed("pdftoppm did not produce a PNG file".to_string()))?;

    let img = Image::from_path(image_path.to_string_lossy().as_ref())
        .map_err(|e| ParseError::OcrFailed(format!("Failed to load image for tesseract: {}", e)))?;

    let args = Args {
        lang: "eng".to_string(),
        ..Default::default()
    };

    let tsv_output = rusty_tesseract::image_to_data(&img, &args).map_err(|e| {
        if let rusty_tesseract::TessError::TesseractNotFoundError = e {
            ParseError::OcrMissingDependency("tesseract".to_string())
        } else {
            ParseError::OcrFailed(format!("Tesseract failed: {:?}", e))
        }
    })?;

    // Group the words by block_num / par_num
    let mut blocks: Vec<Block> = Vec::new();
    let mut current_block: Option<Block> = None;
    let mut current_block_id = -1;
    let mut current_line_id = -1;

    for row in tsv_output.data {
        // Only level 5 represents words
        if row.level != 5 {
            continue;
        }

        // Skip empty text (tesseract sometimes outputs empty words)
        let text = row.text.trim();
        if text.is_empty() {
            continue;
        }

        // Filter out hallucinated noise using Tesseract's confidence score (0-100)
        // A threshold of 40 is a principled way to discard margin artifacts or smudges
        if row.conf < 40.0 {
            continue;
        }

        let block_id = row.block_num;
        let line_id = row.line_num;

        if block_id != current_block_id {
            if let Some(cb) = current_block.take() {
                if !cb.text().is_empty() {
                    blocks.push(cb);
                }
            }
            current_block_id = block_id;
            current_line_id = line_id;

            current_block = Some(Block::Text {
                text: text.to_string(),
                spans: Vec::new(),
                bbox: [
                    row.left as f64,
                    row.top as f64,
                    (row.left + row.width) as f64,
                    (row.top + row.height) as f64,
                ],
                section_id: "body".to_string(),
                block_role: None,
                source: "ocr".to_string(),
            });
        } else if let Some(ref mut cb) = current_block {
            if line_id != current_line_id {
                cb.text_mut().unwrap().push('\n');
                current_line_id = line_id;
            } else {
                cb.text_mut().unwrap().push(' ');
            }
            cb.text_mut().unwrap().push_str(text);

            cb.bbox_mut()[0] = cb.bbox_mut()[0].min(row.left as f64);
            cb.bbox_mut()[1] = cb.bbox_mut()[1].min(row.top as f64);
            cb.bbox_mut()[2] = cb.bbox_mut()[2].max((row.left + row.width) as f64);
            cb.bbox_mut()[3] = cb.bbox_mut()[3].max((row.top + row.height) as f64);
        }
    }

    if let Some(cb) = current_block.take() {
        if !cb.text().is_empty() {
            blocks.push(cb);
        }
    }

    // We need to invert the Y-axis to match PDF space (where y=0 is at the bottom).
    // We also scale from 300 DPI to 72 DPI. (72.0 / 300.0 = 0.24)
    let img_reader = image::io::Reader::open(&image_path).map_err(|e| {
        ParseError::OcrFailed(format!("Failed to open image for dimensions: {}", e))
    })?;
    let img_dimensions = img_reader
        .into_dimensions()
        .map_err(|e| ParseError::OcrFailed(format!("Failed to decode image dimensions: {}", e)))?;
    let page_height_pt = img_dimensions.1 as f64 * 0.24;

    let mut filtered_blocks = Vec::new();
    for mut b in blocks {
        b.bbox_mut()[0] *= 0.24;
        b.bbox_mut()[1] *= 0.24;
        b.bbox_mut()[2] *= 0.24;
        b.bbox_mut()[3] *= 0.24;

        // Invert Y-axis: new_y = page_height_pt - old_y
        let top = page_height_pt - b.bbox_mut()[1];
        let bottom = page_height_pt - b.bbox_mut()[3];

        // Ensure bbox is [x0, y0, x1, y1] where x0 < x1 and y0 < y1
        b.bbox_mut()[1] = bottom;
        b.bbox_mut()[3] = top;

        filtered_blocks.push(b);
    }

    Ok(filtered_blocks)
}
