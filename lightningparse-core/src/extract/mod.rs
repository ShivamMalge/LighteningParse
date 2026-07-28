//! Tier 1: Digital-native text extraction.
//!
//! Parses PDFs with embedded text layers and extracts text blocks
//! with bounding boxes, page numbers, and section metadata.
//! Each BT...ET text object in the content stream becomes one Block.

use std::collections::HashMap;


use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId};
use rayon::prelude::*;

use crate::errors::ParseError;
use crate::output::{Block, Page};

// ── Public API ──────────────────────────────────────────────────

/// Extract structured text from a digital-native PDF loaded into memory.
///
/// Returns a [`ParseResult`] matching the JSON schema in ARCHITECTURE.md §3.1.
/// Pages are processed in parallel via `rayon` and then sorted by page number
/// to guarantee deterministic document-order output.
/// All pages are treated as Tier 1 (digital-native). Header/footer tagging
/// and OCR fallback are not wired up yet (Phase 4 / Phase 5).
pub fn extract_text(doc: &Document) -> Result<Vec<(u32, Vec<Block>, usize)>, ParseError> {
    let pages_map = doc.get_pages(); // BTreeMap<u32, ObjectId>

    // Collect entries so rayon can partition them across threads.
    let entries: Vec<(u32, ObjectId)> = pages_map.iter().map(|(&n, &id)| (n, id)).collect();

    // Extract pages in parallel; collect results and propagate first error.
    let mut results: Vec<(u32, Vec<Block>, usize)> = entries
        .par_iter()
        .map(|&(page_num, page_id)| {
            let page = extract_page(doc, page_num, page_id)?;
            let total_chars = page.blocks.iter().map(|b| b.text.trim().chars().count()).sum();
            Ok((page.page_num, page.blocks, total_chars))
        })
        .collect::<Result<Vec<(u32, Vec<Block>, usize)>, ParseError>>()?;

    // Sort by page_num — parallel execution may reorder results.
    results.sort_by_key(|p| p.0);

    Ok(results)
}

// ── Per-page extraction ─────────────────────────────────────────

fn extract_page(doc: &Document, page_num: u32, page_id: ObjectId) -> Result<Page, ParseError> {
    // Get the content stream bytes (lopdf decompresses + concatenates arrays).
    let content_bytes = match doc.get_page_content(page_id) {
        Ok(data) => data,
        Err(_) => return Ok(Page { page_num, blocks: vec![] }), // no content = blank page
    };

    if content_bytes.is_empty() {
        return Ok(Page { page_num, blocks: vec![] });
    }

    let content = Content::decode(&content_bytes)
        .map_err(|e| ParseError::CorruptPdf(format!("Page {page_num} content stream: {e}")))?;

    let page_obj = doc.get_object(page_id).map_err(|e| ParseError::CorruptPdf(e.to_string()))?;
    let page_dict = page_obj.as_dict().map_err(|_| ParseError::CorruptPdf("Page is not a dictionary".into()))?;
    let resources = get_resources(doc, page_dict);

    let font_map = build_font_map_from_resources(doc, resources);
    let xobjs_dict = resources
        .and_then(|r| r.get(b"XObject").ok())
        .and_then(|x| resolve(doc, x).ok())
        .and_then(|x| x.as_dict().ok());

    let mut raw_blocks = Vec::new();
    process_operations(doc, &content.operations, &font_map, xobjs_dict, IDENTITY, 0, &mut raw_blocks);

    let blocks: Vec<Block> = raw_blocks
        .into_iter()
        .filter(|b| !b.text.trim().is_empty())
        .map(|b| {
            let (min_x, min_y, max_x, max_y) = b.finalise_bbox();
            Block {
                text: b.text,
                bbox: [min_x, min_y, max_x, max_y],
                section_id: "body".into(), // Phase 1: everything is "body"
                source: "digital".into(),
            }
        })
        .collect();

    Ok(Page { page_num, blocks })
}

// ── Font encoding ───────────────────────────────────────────────

/// How to decode character-code bytes → Unicode for one font.
#[derive(Clone)]
enum FontDecoder {
    /// Windows-1252 / WinAnsiEncoding (most common for Western PDFs).
    WinAnsi,
    /// ToUnicode CMap — the most reliable mapping when present.
    CMap(HashMap<Vec<u8>, String>),
    /// Fallback: interpret bytes as Latin-1.
    Fallback,
}

#[derive(Clone)]
struct FontInfo {
    decoder: FontDecoder,
    first_char: u16,
    widths: Option<Vec<f64>>,
}

type FontMap = HashMap<Vec<u8>, FontInfo>;

/// Helper to extract the Resources dictionary from a page or Form XObject.
fn get_resources<'a>(doc: &'a Document, dict: &'a lopdf::Dictionary) -> Option<&'a lopdf::Dictionary> {
    let resources_obj = dict
        .get(b"Resources")
        .ok()
        .or_else(|| {
            let parent_ref = dict.get(b"Parent").ok()?;
            let parent = resolve(doc, parent_ref).ok()?;
            let parent_dict = parent.as_dict().ok()?;
            parent_dict.get(b"Resources").ok()
        })?;
    resolve(doc, resources_obj).ok()?.as_dict().ok()
}

/// Build a font-name → decoder map from a Resources dict.
fn build_font_map_from_resources(doc: &Document, resources: Option<&lopdf::Dictionary>) -> FontMap {
    let mut map = FontMap::new();

    let fonts_dict = resources
        .and_then(|r| r.get(b"Font").ok())
        .and_then(|f| resolve(doc, f).ok())
        .and_then(|f| f.as_dict().ok());

    if let Some(fonts) = fonts_dict {
        for (name, obj) in fonts.iter() {
            let info = build_font_info(doc, obj);
            map.insert(name.clone(), info);
        }
    }

    map
}

fn build_font_info(doc: &Document, font_obj: &Object) -> FontInfo {
    let resolved = match resolve(doc, font_obj) {
        Ok(o) => o,
        Err(_) => return FontInfo { decoder: FontDecoder::Fallback, first_char: 0, widths: None },
    };
    let font_dict = match resolved.as_dict() {
        Ok(d) => d,
        Err(_) => return FontInfo { decoder: FontDecoder::Fallback, first_char: 0, widths: None },
    };

    let decoder = build_font_decoder_from_dict(doc, font_dict);

    let first_char = font_dict.get(b"FirstChar").ok()
        .and_then(|o| resolve(doc, o).ok())
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as u16;

    let widths = font_dict.get(b"Widths").ok()
        .and_then(|o| resolve(doc, o).ok())
        .and_then(|o| o.as_array().ok())
        .map(|arr| {
            arr.iter()
                .map(|item| resolve(doc, item).ok().map(num).unwrap_or(0.0))
                .collect::<Vec<f64>>()
        });

    FontInfo { decoder, first_char, widths }
}

/// Determine the best decoder for a single font dictionary.
fn build_font_decoder_from_dict(doc: &Document, font_dict: &lopdf::Dictionary) -> FontDecoder {
    // 1. ToUnicode CMap is the most reliable when present.
    if let Ok(tu) = font_dict.get(b"ToUnicode") {
        if let Some(data) = get_stream_content(doc, tu) {
            let cmap = parse_to_unicode_cmap(&data);
            if !cmap.is_empty() {
                return FontDecoder::CMap(cmap);
            }
        }
    }

    // 2. Explicit Encoding name.
    if let Ok(enc) = font_dict.get(b"Encoding") {
        if let Ok(Object::Name(ref n)) = resolve(doc, enc) {
            if n == b"WinAnsiEncoding" || n == b"MacRomanEncoding" {
                return FontDecoder::WinAnsi; // MacRoman ≈ WinAnsi for ASCII
            }
            // Encoding could also be a dict with /Differences — skip for Phase 1.
        }
    }

    // 3. Standard base fonts default to WinAnsi.
    if let Ok(Object::Name(ref base)) = font_dict.get(b"BaseFont") {
        let name = String::from_utf8_lossy(base);
        if name.contains("Helvetica")
            || name.contains("Arial")
            || name.contains("Times")
            || name.contains("Courier")
            || name.contains("Symbol") // not exactly WinAnsi, but ASCII-safe
        {
            return FontDecoder::WinAnsi;
        }
    }

    FontDecoder::Fallback
}

// ── ToUnicode CMap parsing ──────────────────────────────────────

/// Parse a ToUnicode CMap stream into a code→string map.
/// Handles `beginbfchar` / `beginbfrange` sections.
fn parse_to_unicode_cmap(data: &[u8]) -> HashMap<Vec<u8>, String> {
    let text = String::from_utf8_lossy(data);
    let mut map = HashMap::new();

    // ── bfchar sections ──
    let mut search_from = 0usize;
    while let Some(start_rel) = text[search_from..].find("beginbfchar") {
        let body_start = search_from + start_rel + "beginbfchar".len();
        if let Some(end_rel) = text[body_start..].find("endbfchar") {
            let section = &text[body_start..body_start + end_rel];
            for line in section.lines() {
                let line = line.trim();
                if !line.starts_with('<') {
                    continue;
                }
                let parts = extract_hex_groups(line);
                if parts.len() >= 2 {
                    if let (Some(src), Some(dst)) = (hex_to_bytes(&parts[0]), hex_to_string(&parts[1]))
                    {
                        map.insert(src, dst);
                    }
                }
            }
            search_from = body_start + end_rel;
        } else {
            break;
        }
    }

    // ── bfrange sections ──
    search_from = 0;
    while let Some(start_rel) = text[search_from..].find("beginbfrange") {
        let body_start = search_from + start_rel + "beginbfrange".len();
        if let Some(end_rel) = text[body_start..].find("endbfrange") {
            let section = &text[body_start..body_start + end_rel];
            for line in section.lines() {
                let line = line.trim();
                if !line.starts_with('<') {
                    continue;
                }
                let parts = extract_hex_groups(line);
                if parts.len() >= 3 {
                    if let (Some(lo), Some(hi), Some(base)) = (
                        hex_to_u32(&parts[0]),
                        hex_to_u32(&parts[1]),
                        hex_to_u32(&parts[2]),
                    ) {
                        let byte_len = parts[0].len().div_ceil(2);
                        for code in lo..=hi {
                            let unicode_val = base + (code - lo);
                            if let Some(ch) = char::from_u32(unicode_val) {
                                let src = code_to_bytes(code, byte_len);
                                map.insert(src, ch.to_string());
                            }
                        }
                    }
                }
            }
            search_from = body_start + end_rel;
        } else {
            break;
        }
    }

    map
}

/// Extract `<hex>` groups from a CMap line, stripping angle brackets.
fn extract_hex_groups(line: &str) -> Vec<String> {
    let mut groups = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('<') {
        if let Some(close) = rest[open + 1..].find('>') {
            groups.push(rest[open + 1..open + 1 + close].to_string());
            rest = &rest[open + 1 + close + 1..];
        } else {
            break;
        }
    }
    groups
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let hex = hex.trim();
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

fn hex_to_u32(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex.trim(), 16).ok()
}

fn hex_to_string(hex: &str) -> Option<String> {
    let bytes = hex_to_bytes(hex)?;
    // CMap destination values are big-endian UTF-16 code units.
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut result = String::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let code = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
        if let Some(ch) = char::from_u32(code as u32) {
            result.push(ch);
        }
        i += 2;
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn code_to_bytes(code: u32, byte_len: usize) -> Vec<u8> {
    let all = code.to_be_bytes(); // 4 bytes
    all[4 - byte_len..].to_vec()
}

// ── Text decoding ───────────────────────────────────────────────

fn decode_text(bytes: &[u8], decoder: &FontDecoder) -> String {
    match decoder {
        FontDecoder::WinAnsi => decode_win_ansi(bytes),
        FontDecoder::CMap(map) => decode_with_cmap(bytes, map),
        FontDecoder::Fallback => decode_win_ansi(bytes), // best-effort
    }
}

fn decode_with_cmap(bytes: &[u8], map: &HashMap<Vec<u8>, String>) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        // Try 2-byte lookup first, then 1-byte.
        let found = if i + 1 < bytes.len() {
            map.get(&bytes[i..i + 2].to_vec()).map(|s| (s.as_str(), 2))
        } else {
            None
        };
        let found = found.or_else(|| map.get(&bytes[i..i + 1].to_vec()).map(|s| (s.as_str(), 1)));
        if let Some((s, advance)) = found {
            result.push_str(s);
            i += advance;
        } else {
            // Unmapped — fallback to WinAnsi for single byte.
            result.push(win_ansi_char(bytes[i]));
            i += 1;
        }
    }
    result
}

fn decode_win_ansi(bytes: &[u8]) -> String {
    bytes.iter().copied().map(win_ansi_char).collect()
}

/// Windows-1252 single-byte → Unicode char.
fn win_ansi_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', // €
        0x82 => '\u{201A}', // ‚
        0x83 => '\u{0192}', // ƒ
        0x84 => '\u{201E}', // „
        0x85 => '\u{2026}', // …
        0x86 => '\u{2020}', // †
        0x87 => '\u{2021}', // ‡
        0x88 => '\u{02C6}', // ˆ
        0x89 => '\u{2030}', // ‰
        0x8A => '\u{0160}', // Š
        0x8B => '\u{2039}', // ‹
        0x8C => '\u{0152}', // Œ
        0x8E => '\u{017D}', // Ž
        0x91 => '\u{2018}', // '
        0x92 => '\u{2019}', // '
        0x93 => '\u{201C}', // "
        0x94 => '\u{201D}', // "
        0x95 => '\u{2022}', // •
        0x96 => '\u{2013}', // –
        0x97 => '\u{2014}', // —
        0x98 => '\u{02DC}', // ˜
        0x99 => '\u{2122}', // ™
        0x9A => '\u{0161}', // š
        0x9B => '\u{203A}', // ›
        0x9C => '\u{0153}', // œ
        0x9E => '\u{017E}', // ž
        0x9F => '\u{0178}', // Ÿ
        0x81 | 0x8D | 0x8F | 0x90 | 0x9D => '\u{FFFD}', // undefined
        _ => b as char, // ASCII + Latin-1 Supplement
    }
}

// ── Content-stream processing ───────────────────────────────────

/// Accumulated text block collected between BT...ET.
pub struct RawBlock {
    pub text: String,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl RawBlock {
    fn new() -> Self {
        Self {
            text: String::new(),
            min_x: f64::MAX,
            min_y: f64::MAX,
            max_x: f64::MIN,
            max_y: f64::MIN,
        }
    }

    fn update_bounds(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x + width);
        self.max_y = self.max_y.max(y + height);
    }

    /// Return normalised bbox; if no text was emitted, default to origin.
    fn finalise_bbox(&self) -> (f64, f64, f64, f64) {
        if self.min_x > self.max_x {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (self.min_x, self.min_y, self.max_x, self.max_y)
        }
    }
}

/// Mutable text-rendering state tracked while walking operators.
struct TextState {
    /// Text matrix [a, b, c, d, tx, ty].
    tm: [f64; 6],
    /// Line matrix (reset at each Td / TD / T* / Tm).
    lm: [f64; 6],
    font_size: f64,
    leading: f64,
    char_spacing: f64,
    word_spacing: f64,
    h_scaling: f64, // percentage, default 100
    font_name: Vec<u8>,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            tm: IDENTITY,
            lm: IDENTITY,
            font_size: 12.0,
            leading: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scaling: 100.0,
            font_name: Vec::new(),
        }
    }
}

fn apply_text_spacing(new_tm: &[f64; 6], ts: &TextState, blocks: &mut Vec<RawBlock>, current: &mut Option<RawBlock>) {
    if let Some(blk) = current.as_mut() {
        if !blk.text.is_empty() {
            let dx = new_tm[4] - ts.tm[4];
            let dy = new_tm[5] - ts.tm[5];
            
            let fs_y = ts.font_size.abs() * if ts.tm[3].abs() > 0.001 { ts.tm[3].abs() } else { 1.0 };
            let thresh_y = if fs_y > 0.1 { fs_y } else { 12.0 };
            
            let fs_x = ts.font_size.abs() * if ts.tm[0].abs() > 0.001 { ts.tm[0].abs() } else { 1.0 };
            let thresh_x = if fs_x > 0.1 { fs_x } else { 12.0 };
            
            if dy > thresh_y || dy < -thresh_y * 2.5 {
                blocks.push(current.take().unwrap());
                *current = Some(RawBlock::new());
            } else if dy.abs() > thresh_y * 0.3 {
                blk.text.push('\n');
            } else if dx > thresh_x * 0.25 {
                blk.text.push(' ');
            }
        }
    }
}

/// Recursively process the operations stream.
fn process_operations(
    doc: &Document,
    ops: &[Operation],
    font_map: &FontMap,
    xobjs_dict: Option<&lopdf::Dictionary>,
    initial_ctm: [f64; 6],
    depth: usize,
    blocks: &mut Vec<RawBlock>,
) {
    if depth > 20 {
        return; // Guard against infinite recursion in malformed PDFs
    }

    let mut ts = TextState::default();
    let mut current: Option<RawBlock> = None;

    // Graphics-state stack (for q / Q). We only track the CTM.
    let mut ctm = initial_ctm;
    let mut gs_stack: Vec<[f64; 6]> = Vec::new();

    for op in ops {
        match op.operator.as_str() {
            // ── graphics state ──
            "q" => gs_stack.push(ctm),
            "Q" => {
                if let Some(saved) = gs_stack.pop() {
                    ctm = saved;
                }
            }
            "cm" if op.operands.len() >= 6 => {
                let m = mat_from_operands(&op.operands);
                ctm = mat_mul(&m, &ctm);
            }

            // ── text object ──
            "BT" => {
                ts = TextState::default();
                current = Some(RawBlock::new());
            }
            "ET" => {
                if let Some(blk) = current.take() {
                    if !blk.text.is_empty() {
                        blocks.push(blk);
                    }
                }
            }

            // ── form xobject ──
            "Do" if !op.operands.is_empty() => {
                if let Object::Name(ref n) = op.operands[0] {
                    if let Some(xobjs) = xobjs_dict {
                        if let Ok(obj_ref) = xobjs.get(n) {
                            if let Ok(resolved) = resolve(doc, obj_ref) {
                                if let Ok(stream) = resolved.as_stream() {
                                    let dict = &stream.dict;
                                    if dict.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or(b"") == b"Form" {
                                        // 1. Get Form's Matrix (default to Identity if missing)
                                        let form_matrix = dict.get(b"Matrix").ok()
                                            .and_then(|o| resolve(doc, o).ok())
                                            .and_then(|o| o.as_array().ok())
                                            .map(|arr| mat_from_operands(arr))
                                            .unwrap_or(IDENTITY);
                                        
                                        // 2. Compose Form Matrix with current CTM
                                        let effective_ctm = mat_mul(&form_matrix, &ctm);
                                        
                                        // 3. Resolve Form's specific resources if present, else inherit
                                        let form_resources = dict.get(b"Resources").ok()
                                            .and_then(|o| resolve(doc, o).ok())
                                            .and_then(|o| o.as_dict().ok());
                                            
                                        let form_font_map = if let Some(res) = form_resources {
                                            build_font_map_from_resources(doc, Some(res))
                                        } else {
                                            font_map.clone()
                                        };
                                        
                                        let form_xobjs_dict = if let Some(res) = form_resources {
                                            res.get(b"XObject").ok()
                                                .and_then(|o| resolve(doc, o).ok())
                                                .and_then(|o| o.as_dict().ok())
                                        } else {
                                            xobjs_dict
                                        };
                                        
                                        // 4. Decode Form stream and recurse
                                        if let Some(form_content_bytes) = get_stream_content(doc, resolved) {
                                            if let Ok(form_content) = lopdf::content::Content::decode(&form_content_bytes) {
                                                // If there's an active text block, push it before entering the form
                                                if let Some(blk) = current.take() {
                                                    if !blk.text.is_empty() {
                                                        blocks.push(blk);
                                                    }
                                                }
                                                
                                                process_operations(
                                                    doc,
                                                    &form_content.operations,
                                                    &form_font_map,
                                                    form_xobjs_dict,
                                                    effective_ctm,
                                                    depth + 1,
                                                    blocks,
                                                );
                                                
                                                // No need to restore text state; 'Do' operates outside text objects.
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── text state ──
            "Tf" if op.operands.len() >= 2 => {
                if let Object::Name(ref n) = op.operands[0] {
                    ts.font_name = n.clone();
                }
                ts.font_size = num(&op.operands[1]);
            }
            "Tc" if !op.operands.is_empty() => ts.char_spacing = num(&op.operands[0]),
            "Tw" if !op.operands.is_empty() => ts.word_spacing = num(&op.operands[0]),
            "TL" if !op.operands.is_empty() => ts.leading = num(&op.operands[0]),
            "Tz" if !op.operands.is_empty() => ts.h_scaling = num(&op.operands[0]),

            // ── text positioning ──
            "Tm" if op.operands.len() >= 6 => {
                let new_tm = mat_from_operands(&op.operands);
                apply_text_spacing(&new_tm, &ts, blocks, &mut current);
                ts.tm = new_tm;
                ts.lm = ts.tm;
            }
            "Td" if op.operands.len() >= 2 => {
                let tx = num(&op.operands[0]);
                let ty = num(&op.operands[1]);
                let new_tm = mat_translate(&ts.lm, tx, ty);
                apply_text_spacing(&new_tm, &ts, blocks, &mut current);
                ts.lm = new_tm;
                ts.tm = ts.lm;
            }
            "TD" if op.operands.len() >= 2 => {
                let tx = num(&op.operands[0]);
                let ty = num(&op.operands[1]);
                ts.leading = -ty;
                let new_tm = mat_translate(&ts.lm, tx, ty);
                apply_text_spacing(&new_tm, &ts, blocks, &mut current);
                ts.lm = new_tm;
                ts.tm = ts.lm;
            }
            "T*" => {
                let new_tm = mat_translate(&ts.lm, 0.0, -ts.leading);
                apply_text_spacing(&new_tm, &ts, blocks, &mut current);
                ts.lm = new_tm;
                ts.tm = ts.lm;
            }

            // ── text showing ──
            "Tj" => {
                if let Some(ref mut blk) = current {
                    if let Some(bytes) = op.operands.first().and_then(string_bytes) {
                        show_string(bytes, &mut ts, &ctm, font_map, blk);
                    }
                }
            }
            "TJ" => {
                if let Some(ref mut blk) = current {
                    if let Some(Object::Array(ref arr)) = op.operands.first() {
                        show_tj_array(arr, &mut ts, &ctm, font_map, blk);
                    }
                }
            }
            "'" => {
                // T* then Tj
                ts.lm = mat_translate(&ts.lm, 0.0, -ts.leading);
                ts.tm = ts.lm;
                if let Some(ref mut blk) = current {
                    if !blk.text.is_empty() {
                        blk.text.push('\n');
                    }
                    if let Some(bytes) = op.operands.first().and_then(string_bytes) {
                        show_string(bytes, &mut ts, &ctm, font_map, blk);
                    }
                }
            }
            "\"" if op.operands.len() >= 3 => {
                ts.word_spacing = num(&op.operands[0]);
                ts.char_spacing = num(&op.operands[1]);
                ts.lm = mat_translate(&ts.lm, 0.0, -ts.leading);
                ts.tm = ts.lm;
                if let Some(ref mut blk) = current {
                    if !blk.text.is_empty() {
                        blk.text.push('\n');
                    }
                    if let Some(bytes) = string_bytes(&op.operands[2]) {
                        show_string(bytes, &mut ts, &ctm, font_map, blk);
                    }
                }
            }

            _ => {} // ignore non-text operators
        }
    }

    // Capture dangling block if ET was missing.
    if let Some(blk) = current {
        if !blk.text.is_empty() {
            blocks.push(blk);
        }
    }
}

/// Decode + emit a simple text string, updating position and bbox.
fn show_string(
    bytes: &[u8],
    ts: &mut TextState,
    ctm: &[f64; 6],
    font_map: &FontMap,
    blk: &mut RawBlock,
) {
    let info = font_map.get(&ts.font_name);
    let decoder = info.map(|i| &i.decoder).unwrap_or(&FontDecoder::Fallback);
    let decoded = decode_text(bytes, decoder);
    let width = calculate_string_width(bytes, info, ts.font_size, ts.h_scaling);
    let advance = width / (ts.h_scaling / 100.0);

    let comp = mat_mul(&ts.tm, ctm);
    let h = ts.font_size.abs();
    let c1 = transform_pt(0.0, 0.0, &comp);
    let c2 = transform_pt(advance, 0.0, &comp);
    let c3 = transform_pt(advance, h, &comp);
    let c4 = transform_pt(0.0, h, &comp);

    let min_x = c1.0.min(c2.0).min(c3.0).min(c4.0);
    let max_x = c1.0.max(c2.0).max(c3.0).max(c4.0);
    let min_y = c1.1.min(c2.1).min(c3.1).min(c4.1);
    let max_y = c1.1.max(c2.1).max(c3.1).max(c4.1);

    blk.update_bounds(min_x, min_y, max_x - min_x, max_y - min_y);
    blk.text.push_str(&decoded);

    // Advance the text position along the unscaled text vector
    ts.tm[4] += advance * ts.tm[0];
    ts.tm[5] += advance * ts.tm[1];
}

/// Process a TJ array: interleaved strings and kerning numbers.
fn show_tj_array(
    arr: &[Object],
    ts: &mut TextState,
    ctm: &[f64; 6],
    font_map: &FontMap,
    blk: &mut RawBlock,
) {
    let info = font_map.get(&ts.font_name);
    let decoder = info.map(|i| &i.decoder).unwrap_or(&FontDecoder::Fallback);

    for item in arr {
        if let Some(bytes) = string_bytes(item) {
            let decoded = decode_text(bytes, decoder);
            let width = calculate_string_width(bytes, info, ts.font_size, ts.h_scaling);
            let advance = width / (ts.h_scaling / 100.0);

            let comp = mat_mul(&ts.tm, ctm);
            let h = ts.font_size.abs();
            let c1 = transform_pt(0.0, 0.0, &comp);
            let c2 = transform_pt(advance, 0.0, &comp);
            let c3 = transform_pt(advance, h, &comp);
            let c4 = transform_pt(0.0, h, &comp);

            let min_x = c1.0.min(c2.0).min(c3.0).min(c4.0);
            let max_x = c1.0.max(c2.0).max(c3.0).max(c4.0);
            let min_y = c1.1.min(c2.1).min(c3.1).min(c4.1);
            let max_y = c1.1.max(c2.1).max(c3.1).max(c4.1);

            blk.update_bounds(min_x, min_y, max_x - min_x, max_y - min_y);
            blk.text.push_str(&decoded);

            ts.tm[4] += advance * ts.tm[0];
            ts.tm[5] += advance * ts.tm[1];
        } else {
            // Kerning adjustment in thousandths of a text-space unit.
            let adj = num(item);
            let displacement = adj * ts.font_size / 1000.0;
            let advance = -displacement / (ts.h_scaling / 100.0);
            ts.tm[4] += advance * ts.tm[0];
            ts.tm[5] += advance * ts.tm[1];
            // Large negative adj (= positive spacing) often means a word gap.
            if adj < -120.0 {
                blk.text.push(' ');
            }
        }
    }
}

/// Calculate the exact width of a string in text space using the PDF font's /Widths array if available.
/// Fallback to 0.5 ems for missing widths.
fn calculate_string_width(bytes: &[u8], font_info: Option<&FontInfo>, font_size: f64, h_scaling: f64) -> f64 {
    let mut total_width_thousandths = 0.0;
    
    for &byte in bytes {
        let code = byte as u16;
        let mut glyph_width = 500.0; // Standard fallback (0.5 ems)
        
        if let Some(info) = font_info {
            if let Some(widths) = &info.widths {
                if code >= info.first_char && (code - info.first_char) < widths.len() as u16 {
                    glyph_width = widths[(code - info.first_char) as usize];
                }
            }
        }
        total_width_thousandths += glyph_width;
    }
    
    (total_width_thousandths / 1000.0) * font_size.abs() * (h_scaling / 100.0)
}

// ── Matrix helpers (2-D affine, stored as [a b c d e f]) ────────

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mat_from_operands(ops: &[Object]) -> [f64; 6] {
    [
        num(&ops[0]),
        num(&ops[1]),
        num(&ops[2]),
        num(&ops[3]),
        num(&ops[4]),
        num(&ops[5]),
    ]
}

/// Multiply two 2-D affine matrices stored as [a b c d e f].
///
/// Treats each as the 3×3:
/// ```text
/// [a  b  0]
/// [c  d  0]
/// [e  f  1]
/// ```
fn mat_mul(a: &[f64; 6], b: &[f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// Pre-multiply the translation [1 0 0 1 tx ty] × m.
fn mat_translate(m: &[f64; 6], tx: f64, ty: f64) -> [f64; 6] {
    [
        m[0],
        m[1],
        m[2],
        m[3],
        tx * m[0] + ty * m[2] + m[4],
        tx * m[1] + ty * m[3] + m[5],
    ]
}

/// Transform a point (x, y) through an affine matrix.
fn transform_pt(x: f64, y: f64, m: &[f64; 6]) -> (f64, f64) {
    (
        m[0] * x + m[2] * y + m[4],
        m[1] * x + m[3] * y + m[5],
    )
}

// ── Object helpers ──────────────────────────────────────────────

/// Follow a Reference one level; return the same object if it isn't a ref.
fn resolve<'a>(doc: &'a Document, obj: &'a Object) -> Result<&'a Object, ParseError> {
    match *obj {
        Object::Reference(id) => doc
            .get_object(id)
            .map_err(|e| ParseError::CorruptPdf(format!("Bad ref {id:?}: {e}"))),
        _ => Ok(obj),
    }
}

/// Extract an f64 from an Integer or Real object (0.0 for anything else).
fn num(obj: &Object) -> f64 {
    match *obj {
        Object::Integer(i) => i as f64,
        Object::Real(f) => f as f64,
        _ => 0.0,
    }
}

/// Get the raw bytes of a String object.
fn string_bytes(obj: &Object) -> Option<&[u8]> {
    match *obj {
        Object::String(ref bytes, _) => Some(bytes),
        _ => None,
    }
}

/// Decompress and return the bytes of a Stream object (following refs).
fn get_stream_content(doc: &Document, obj: &Object) -> Option<Vec<u8>> {
    let resolved = resolve(doc, obj).ok()?;
    if let Object::Stream(ref stream) = *resolved {
        let mut s = stream.clone();
        // decompress() may fail on already-decompressed or unsupported filters.
        s.decompress();
        Some(s.content)
    } else {
        None
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, StringFormat, Stream};

    // ── helpers to build fixture PDFs programmatically ──

    /// Build a minimal valid PDF with the given per-page text entries.
    /// Each entry is (text, x_position, y_position).
    fn make_pdf(pages: &[(&str, f64, f64)]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_id),
            },
        });

        let mut page_ids = Vec::new();
        for &(text, x, y) in pages {
            let ops = vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
                ),
                Operation::new("Td", vec![Object::Real(x as f32), Object::Real(y as f32)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(text.as_bytes().to_vec(), StringFormat::Literal)],
                ),
                Operation::new("ET", vec![]),
            ];
            let content = Content { operations: ops };
            let content_bytes = content.encode().unwrap();
            let stream_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));

            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(612),
                    Object::Integer(792),
                ],
                "Contents" => Object::Reference(stream_id),
                "Resources" => Object::Reference(resources_id),
            });
            page_ids.push(page_id);
        }

        let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => Object::Integer(page_ids.len() as i64),
        });
        for &pid in &page_ids {
            if let Ok(obj) = doc.objects.get(&pid).ok_or(()) {
                if let Object::Dictionary(ref d) = *obj {
                    let mut d2 = d.clone();
                    d2.set("Parent", Object::Reference(pages_id));
                    doc.objects.insert(pid, Object::Dictionary(d2));
                }
            }
        }
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    // Helper for tests to emulate the old API
    fn extract_text_for_test(pdf_bytes: &[u8]) -> Result<crate::output::ParseResult, ParseError> {
        if pdf_bytes.is_empty() {
            return Err(ParseError::CorruptPdf("PDF data is empty".into()));
        }
        let doc = Document::load_mem(pdf_bytes).map_err(|e| ParseError::CorruptPdf(e.to_string()))?;
        let pages_tuples = extract_text(&doc)?;
        let mut pages = Vec::new();
        for (page_num, blocks, _) in pages_tuples {
            pages.push(crate::output::Page { page_num, blocks });
        }
        Ok(crate::output::ParseResult {
            pages,
            metadata: crate::output::DocumentMetadata {
                tier: "digital".to_string(),
                page_count: doc.get_pages().len() as u32,
                parse_time_ms: 0,
            }
        })
    }

    /// Build a valid PDF whose single page has an empty content stream.
    fn make_empty_page_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");

        let stream_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ],
            "Contents" => Object::Reference(stream_id),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        });
        {
            let obj = doc.objects.get(&page_id).unwrap().clone();
            if let Object::Dictionary(mut d) = obj {
                d.set("Parent", Object::Reference(pages_id));
                doc.objects.insert(page_id, Object::Dictionary(d));
            }
        }
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    // ── actual tests ──

    #[test]
    fn test_valid_single_page() {
        let pdf = make_pdf(&[("Hello World", 72.0, 700.0)]);
        let result = extract_text_for_test(&pdf).expect("should parse valid PDF");

        assert_eq!(result.metadata.page_count, 1);
        assert_eq!(result.metadata.tier, "digital");
        assert_eq!(result.pages.len(), 1);
        assert_eq!(result.pages[0].page_num, 1);
        assert!(!result.pages[0].blocks.is_empty(), "should have at least one block");

        let block = &result.pages[0].blocks[0];
        assert!(
            block.text.contains("Hello World"),
            "block text '{}' should contain 'Hello World'",
            block.text,
        );
        assert_eq!(block.section_id, "body");
        assert_eq!(block.source, "digital");
        // bbox should be non-degenerate
        assert!(block.bbox[2] > block.bbox[0], "x1 > x0");
        assert!(block.bbox[3] > block.bbox[1], "y1 > y0");
    }

    #[test]
    fn test_multi_page() {
        let pdf = make_pdf(&[
            ("Page One", 72.0, 700.0),
            ("Page Two", 72.0, 700.0),
            ("Page Three", 72.0, 700.0),
        ]);
        let result = extract_text_for_test(&pdf).expect("should parse multi-page PDF");

        assert_eq!(result.metadata.page_count, 3);
        assert_eq!(result.pages.len(), 3);

        // Pages should be in order.
        assert_eq!(result.pages[0].page_num, 1);
        assert_eq!(result.pages[1].page_num, 2);
        assert_eq!(result.pages[2].page_num, 3);

        assert!(result.pages[0].blocks[0].text.contains("Page One"));
        assert!(result.pages[1].blocks[0].text.contains("Page Two"));
        assert!(result.pages[2].blocks[0].text.contains("Page Three"));
    }

    #[test]
    fn test_empty_page() {
        let pdf = make_empty_page_pdf();
        let result = extract_text_for_test(&pdf).expect("empty page is valid");

        assert_eq!(result.metadata.page_count, 1);
        assert!(
            result.pages[0].blocks.is_empty(),
            "empty page should have no blocks",
        );
    }

    #[test]
    fn test_empty_bytes_returns_error() {
        let result = extract_text_for_test(b"");
        assert!(result.is_err(), "empty bytes must be Err");
        match result.unwrap_err() {
            ParseError::CorruptPdf(_) => {} // expected
            other => panic!("expected CorruptPdf, got {other:?}"),
        }
    }

    #[test]
    fn test_corrupted_bytes_returns_error() {
        let garbage = b"this is not a PDF at all";
        let result = extract_text_for_test(garbage);
        assert!(result.is_err(), "garbage bytes must be Err");
        match result.unwrap_err() {
            ParseError::CorruptPdf(_) => {}
            other => panic!("expected CorruptPdf, got {other:?}"),
        }
    }

    #[test]
    fn test_truncated_pdf_returns_error() {
        let full = make_pdf(&[("Truncated", 72.0, 700.0)]);
        // Take only the first 50 bytes — a truncated header.
        let truncated = &full[..50.min(full.len())];
        let result = extract_text_for_test(truncated);
        assert!(result.is_err(), "truncated PDF must be Err");
    }

    #[test]
    fn test_random_bytes_never_panics() {
        // A spread of deterministic "random" byte patterns.
        let patterns: Vec<Vec<u8>> = vec![
            vec![0xFF; 100],
            vec![0x00; 100],
            (0u8..=255).collect(),
            b"%PDF-1.4\ngarbage".to_vec(),
            b"%PDF-1.4 0 obj\n<<>>stream\nendstream endobj".to_vec(),
        ];
        for pat in &patterns {
            // Must never panic — Ok or Err are both fine.
            let _ = extract_text_for_test(pat);
        }
    }

    #[test]
    fn test_json_schema_matches_architecture() {
        let pdf = make_pdf(&[("Schema test", 100.0, 500.0)]);
        let result = extract_text_for_test(&pdf).unwrap();
        let json_str = serde_json::to_string(&result).expect("serialization must work");
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Top-level keys
        assert!(val.get("pages").is_some());
        assert!(val.get("metadata").is_some());

        // Metadata shape
        let meta = &val["metadata"];
        assert!(meta["tier"].is_string());
        assert!(meta["page_count"].is_number());
        assert!(meta["parse_time_ms"].is_number());

        // Page / block shape
        let page = &val["pages"][0];
        assert!(page["page_num"].is_number());
        let blk = &page["blocks"][0];
        assert!(blk["text"].is_string());
        assert!(blk["bbox"].is_array());
        assert_eq!(blk["bbox"].as_array().unwrap().len(), 4);
        assert!(blk["section_id"].is_string());
        assert!(blk["source"].is_string());
    }

    #[test]
    fn test_tj_array_word_spacing() {
        // Build a PDF using TJ with kerning adjustments
        let mut doc = Document::with_version("1.5");
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        });

        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Integer(12)]),
            Operation::new("Td", vec![Object::Real(72.0), Object::Real(700.0)]),
            Operation::new(
                "TJ",
                vec![Object::Array(vec![
                    Object::String(b"Hel".to_vec(), StringFormat::Literal),
                    Object::Integer(-10),  // small kerning
                    Object::String(b"lo".to_vec(), StringFormat::Literal),
                    Object::Integer(-200), // large gap → space
                    Object::String(b"World".to_vec(), StringFormat::Literal),
                ])],
            ),
            Operation::new("ET", vec![]),
        ];
        let content = Content { operations: ops };
        let content_bytes = content.encode().unwrap();
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)],
            "Contents" => Object::Reference(stream_id),
            "Resources" => Object::Reference(resources_id),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        });
        {
            let obj = doc.objects.get(&page_id).unwrap().clone();
            if let Object::Dictionary(mut d) = obj {
                d.set("Parent", Object::Reference(pages_id));
                doc.objects.insert(page_id, Object::Dictionary(d));
            }
        }
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let result = extract_text_for_test(&buf).unwrap();
        let text = &result.pages[0].blocks[0].text;
        assert!(text.contains("Hello"), "should contain 'Hello', got '{text}'");
        assert!(text.contains("World"), "should contain 'World', got '{text}'");
    }
}
