import os
import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont
from fpdf import FPDF
import math

CORPUS_DIR = os.path.join("benchmarks", "corpus")
os.makedirs(CORPUS_DIR, exist_ok=True)

# 1. Generate digital native Word-like export
def generate_digital_word_export():
    pdf = FPDF(format="A4")
    pdf.add_page()
    pdf.set_font("Helvetica", size=12)
    
    # Header
    pdf.set_font("Helvetica", style="B", size=16)
    pdf.cell(0, 10, "CONFIDENTIAL MEMORANDUM", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(5)
    
    # Body
    pdf.set_font("Helvetica", size=12)
    text = (
        "To: Engineering Team\n"
        "From: Architecture Group\n"
        "Date: October 2023\n"
        "Subject: RAG Pipeline Optimization Strategy\n\n"
        "Executive Summary\n"
        "The current PDF parsing library has proven to be a bottleneck for our ingestion pipeline. "
        "Due to the Python Global Interpreter Lock (GIL), processing pages concurrently yields no "
        "measurable speedup. We propose writing a Rust core that extracts raw text and routes "
        "scanned pages to Tesseract, effectively bypassing Python until the final chunking stage.\n\n"
        "Implementation Details\n"
        "1. Extract digital-native text directly using lopdf.\n"
        "2. Identify scanned pages (no text objects) and pass them to OCR.\n"
        "3. Provide section metadata to the downstream chunker to automatically discard headers and footers.\n"
    )
    pdf.multi_cell(0, 8, text)
    
    # Footer
    pdf.set_y(-15)
    pdf.set_font("Helvetica", style="I", size=8)
    pdf.cell(0, 10, "Page 1 of 1 - Internal Use Only", align="C")
    
    output_path = os.path.join(CORPUS_DIR, "digital_word_export.pdf")
    pdf.output(output_path)
    print(f"Generated {output_path}")

def create_document_image(text_lines):
    # Create high-res blank page
    img = Image.new("RGB", (1700, 2200), "white")
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("arial.ttf", 60)
    except IOError:
        font = ImageFont.load_default()
    
    y = 200
    for line in text_lines:
        draw.text((200, y), line, fill="black", font=font)
        y += 80
    return img

def apply_phone_photo_effects(img, rotate_angle=0, shadow_intensity=0.5, blur=0, perspective_warp=False):
    # Convert PIL to CV2
    cv_img = np.array(img)
    cv_img = cv_img[:, :, ::-1].copy() # RGB to BGR
    h, w = cv_img.shape[:2]
    
    # 1. Perspective Warp (simulate holding phone at an angle)
    if perspective_warp:
        pts1 = np.float32([[0, 0], [w, 0], [0, h], [w, h]])
        pts2 = np.float32([[50, 100], [w-100, 50], [0, h], [w-50, h-100]])
        matrix = cv2.getPerspectiveTransform(pts1, pts2)
        cv_img = cv2.warpPerspective(cv_img, matrix, (w, h), borderMode=cv2.BORDER_CONSTANT, borderValue=(200,200,200))

    # 2. Rotation
    if rotate_angle != 0:
        center = (w // 2, h // 2)
        M = cv2.getRotationMatrix2D(center, rotate_angle, 1.0)
        cv_img = cv2.warpAffine(cv_img, M, (w, h), borderMode=cv2.BORDER_CONSTANT, borderValue=(220, 220, 210))
    
    # 3. Add shadow/lighting gradient
    if shadow_intensity > 0:
        gradient = np.zeros((h, w), dtype=np.float32)
        for i in range(h):
            # linear gradient
            gradient[i, :] = 1.0 - (shadow_intensity * (i / h))
        gradient = cv2.merge([gradient, gradient, gradient])
        cv_img = cv_img.astype(np.float32) * gradient
        cv_img = np.clip(cv_img, 0, 255).astype(np.uint8)
        
    # 4. Blur (out of focus)
    if blur > 0:
        cv_img = cv2.GaussianBlur(cv_img, (blur, blur), 0)
        
    # 5. Add noise (grain)
    noise = np.random.normal(0, 10, cv_img.shape).astype(np.float32)
    cv_img = cv_img.astype(np.float32) + noise
    cv_img = np.clip(cv_img, 0, 255).astype(np.uint8)

    # Convert back to PIL
    cv_img = cv_img[:, :, ::-1] # BGR to RGB
    return Image.fromarray(cv_img)

def generate_scanned_pdfs():
    text1 = [
        "INVOICE #992381",
        "",
        "Date: 2023-11-01",
        "Billed To: Acme Corp",
        "",
        "Services Rendered: PDF Extraction API",
        "Amount Due: $1,200.00",
        "",
        "Please pay within 30 days.",
        "Thank you for your business!"
    ]
    
    text2 = [
        "Meeting Minutes - Q3 Planning",
        "",
        "Attendees: Alice, Bob, Charlie",
        "",
        "1. Discussed OCR accuracy on skewed pages.",
        "2. Decided to implement Tesseract fallback.",
        "3. Action Item: Bob to research Rust bindings.",
        "",
        "Meeting adjourned at 14:00."
    ]

    # Generate Phone Photo 1: Slight rotation, strong shadow, some blur
    img1 = create_document_image(text1)
    img1_fx = apply_phone_photo_effects(img1, rotate_angle=2.5, shadow_intensity=0.4, blur=3, perspective_warp=False)
    img1_path = os.path.join(CORPUS_DIR, "phone_photo_invoice.jpg")
    img1_fx.save(img1_path, quality=80)
    
    # Save as PDF
    pdf1 = FPDF(unit="pt", format=[img1_fx.width, img1_fx.height])
    pdf1.add_page()
    pdf1.image(img1_path, 0, 0, img1_fx.width, img1_fx.height)
    pdf1_out = os.path.join(CORPUS_DIR, "phone_photo_invoice.pdf")
    pdf1.output(pdf1_out)
    print(f"Generated {pdf1_out}")

    # Generate Phone Photo 2: Perspective warp, high noise, no rotation
    img2 = create_document_image(text2)
    img2_fx = apply_phone_photo_effects(img2, rotate_angle=0, shadow_intensity=0.2, blur=5, perspective_warp=True)
    img2_path = os.path.join(CORPUS_DIR, "phone_photo_minutes.jpg")
    img2_fx.save(img2_path, quality=70)
    
    # Save as PDF
    pdf2 = FPDF(unit="pt", format=[img2_fx.width, img2_fx.height])
    pdf2.add_page()
    pdf2.image(img2_path, 0, 0, img2_fx.width, img2_fx.height)
    pdf2_out = os.path.join(CORPUS_DIR, "phone_photo_minutes.pdf")
    pdf2.output(pdf2_out)
    print(f"Generated {pdf2_out}")

    # Clean up jpgs
    os.remove(img1_path)
    os.remove(img2_path)

def generate_ieee_template_placeholder():
    """Generate an 8-page IEEE two-column layout placeholder PDF.
    
    This replaces draft10.pdf as the Tier 1 multi-page fixture.
    Structure: title page with abstract, then body pages with two-column
    sections, references, and page numbers.
    """
    pdf = FPDF(format="letter")
    pdf.set_auto_page_break(auto=True, margin=25)

    # Reusable body text blocks
    body_paragraphs = [
        "Recent advances in document parsing have demonstrated that parallel extraction "
        "architectures can significantly outperform traditional sequential approaches. In this "
        "work, we present a hybrid system that combines digital-native text extraction with "
        "optical character recognition for scanned pages, achieving consistent throughput "
        "improvements across diverse document types.",

        "The proposed pipeline operates in two tiers. Tier 1 handles digitally-authored PDFs "
        "by directly parsing the content stream operators, reconstructing glyph positions from "
        "the text matrix and font metrics. Tier 2 identifies pages lacking embedded text layers "
        "and routes them through an OCR engine with confidence-based filtering to suppress noise "
        "from margin artifacts and scanner distortion.",

        "Experimental evaluation on a corpus of 50 documents spanning academic papers, invoices, "
        "and government reports shows a median speedup of 47x over baseline Python libraries for "
        "digital-native extraction, with no measurable accuracy regression. The OCR fallback path "
        "correctly recovers text from 94 percent of scanned pages with confidence above the "
        "filtering threshold.",

        "Font metric resolution is critical for accurate bounding box computation. Simple fonts "
        "use a Widths array indexed by character code minus FirstChar. Composite CID fonts require "
        "parsing the W array from the descendant CIDFont dictionary, with DW providing a default "
        "width for any CID not explicitly listed. Our implementation handles both Identity-H and "
        "Identity-V encodings for CJK text.",

        "Header and footer detection uses a heuristic approach: text blocks appearing in the top "
        "or bottom 12 percent of the page, with content that repeats across three or more pages, "
        "are tagged with section_id header or footer. This tagging preserves the content in the "
        "output while allowing downstream consumers to filter it as needed.",

        "Reading order reconstruction sorts blocks by vertical position first, then applies a "
        "column-detection heuristic for multi-column layouts. Blocks are grouped into columns "
        "based on their horizontal midpoint relative to the page center, ensuring that two-column "
        "academic papers are read left-column-first, then right-column.",

        "The FFI boundary between Rust and Python uses JSON serialization. While this adds a "
        "small constant overhead, profiling confirms it accounts for less than 2 percent of total "
        "parse time even on large documents. The GIL is released during all Rust parsing operations "
        "to allow true parallelism in multi-threaded Python applications.",

        "Future work includes structured table extraction, where grid-aligned text blocks would be "
        "detected and output as row-column structures rather than flattened text. This requires "
        "identifying consistent column x-positions across adjacent rows, a problem well-suited to "
        "the existing block-level coordinate data.",
    ]

    section_titles = [
        "I. Introduction",
        "II. System Architecture",
        "III. Tier 1: Digital-Native Extraction",
        "IV. Tier 2: OCR Fallback",
        "V. Font Metrics and Bounding Boxes",
        "VI. Header/Footer Detection",
        "VII. Reading Order Reconstruction",
        "VIII. Experimental Results",
        "IX. Discussion",
        "X. Related Work",
        "XI. Conclusion",
        "XII. Future Work",
    ]

    references = [
        "[1] A. Smith et al., \"Fast PDF parsing with Rust,\" Proc. SIGMOD, 2023.",
        "[2] B. Jones, \"Optical character recognition: A survey,\" ACM Computing Surveys, vol. 55, 2022.",
        "[3] C. Lee and D. Kim, \"CID font metrics in the PDF specification,\" Tech. Report, 2021.",
        "[4] E. Brown, \"Parallel document processing pipelines,\" IEEE TKDE, vol. 34, 2022.",
        "[5] F. Garcia et al., \"Header detection in scientific documents,\" ICDAR, 2023.",
        "[6] G. Wang, \"Reading order in multi-column layouts,\" DAS Workshop, 2022.",
        "[7] H. Chen, \"RAG pipelines for enterprise search,\" NAACL Industry Track, 2023.",
        "[8] I. Patel and J. Kumar, \"Benchmarking PDF libraries,\" arXiv:2301.04567, 2023.",
    ]

    # --- Page 1: Title + Abstract ---
    pdf.add_page()
    pdf.set_font("Helvetica", style="B", size=16)
    pdf.cell(0, 12, "LightningParse: A Hybrid Rust-Python Pipeline", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.cell(0, 12, "for High-Throughput PDF Text Extraction", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(4)
    pdf.set_font("Helvetica", size=10)
    pdf.cell(0, 8, "Anonymous Authors", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.cell(0, 8, "Department of Computer Science, Example University", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(6)

    # Abstract
    pdf.set_font("Helvetica", style="B", size=10)
    pdf.cell(0, 8, "Abstract", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", style="I", size=9)
    abstract_text = (
        "We present LightningParse, a document parsing system that combines a Rust core for "
        "parallel text extraction with a Python API layer for downstream NLP integration. The "
        "system routes each page independently through either digital-native extraction or OCR "
        "fallback based on content detection, achieving a median 47x speedup over existing Python "
        "libraries on digital-native PDFs while maintaining equivalent accuracy. We describe the "
        "architecture, font metric handling for CID composite fonts, and heuristic-based header "
        "and footer detection. Evaluation on a diverse corpus demonstrates robust performance "
        "across academic papers, invoices, and mixed-content documents."
    )
    pdf.multi_cell(0, 5, abstract_text, align="J")
    pdf.ln(4)

    # Two-column body starts
    col_width = 88
    col_gap = 10
    left_x = pdf.l_margin
    right_x = left_x + col_width + col_gap
    section_idx = 0
    para_idx = 0

    def add_page_number():
        pdf.set_y(-20)
        pdf.set_font("Helvetica", size=9)
        pdf.cell(0, 10, str(pdf.page_no()), align="C")

    def write_two_column_content(start_section, start_para, num_sections=3):
        nonlocal section_idx, para_idx
        section_idx = start_section
        para_idx = start_para
        
        for col_x in [left_x, right_x]:
            pdf.set_xy(col_x, pdf.get_y() if col_x == left_x else top_y)
            sections_in_col = 0
            while sections_in_col < num_sections and section_idx < len(section_titles):
                # Section heading
                if pdf.get_y() > 230:
                    break
                pdf.set_x(col_x)
                pdf.set_font("Helvetica", style="B", size=10)
                pdf.multi_cell(col_width, 6, section_titles[section_idx], new_x="LMARGIN", new_y="NEXT")
                pdf.set_x(col_x)
                pdf.set_font("Helvetica", size=9)
                if para_idx < len(body_paragraphs):
                    pdf.multi_cell(col_width, 5, body_paragraphs[para_idx], align="J", new_x="LMARGIN", new_y="NEXT")
                    para_idx += 1
                pdf.ln(2)
                section_idx += 1
                sections_in_col += 1

    # First two-column section on page 1
    top_y = pdf.get_y()
    write_two_column_content(0, 0, num_sections=2)
    add_page_number()

    # Pages 2-7: continue body content
    for page_num in range(2, 8):
        pdf.add_page()
        top_y = pdf.t_margin
        remaining_sections = min(3, len(section_titles) - section_idx)
        remaining_paras = min(3, len(body_paragraphs) - para_idx)
        
        for col_x in [left_x, right_x]:
            pdf.set_xy(col_x, top_y)
            items = 0
            while items < 2 and section_idx < len(section_titles):
                if pdf.get_y() > 230:
                    break
                pdf.set_x(col_x)
                pdf.set_font("Helvetica", style="B", size=10)
                pdf.multi_cell(col_width, 6, section_titles[section_idx], new_x="LMARGIN", new_y="NEXT")
                pdf.set_x(col_x)
                pdf.set_font("Helvetica", size=9)
                if para_idx < len(body_paragraphs):
                    pdf.multi_cell(col_width, 5, body_paragraphs[para_idx], align="J", new_x="LMARGIN", new_y="NEXT")
                    para_idx += 1
                # Add a second paragraph for density
                if para_idx < len(body_paragraphs):
                    pdf.set_x(col_x)
                    pdf.multi_cell(col_width, 5, body_paragraphs[para_idx], align="J", new_x="LMARGIN", new_y="NEXT")
                    para_idx += 1
                pdf.ln(2)
                section_idx += 1
                items += 1
            # Wrap around paragraph index for more content
            if para_idx >= len(body_paragraphs):
                para_idx = 0
        
        add_page_number()

    # Page 8: References
    pdf.add_page()
    pdf.set_font("Helvetica", style="B", size=12)
    pdf.cell(0, 10, "References", align="C", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(4)
    pdf.set_font("Helvetica", size=9)
    for ref in references:
        pdf.multi_cell(0, 5, ref, new_x="LMARGIN", new_y="NEXT")
        pdf.ln(2)
    add_page_number()

    output_path = os.path.join(CORPUS_DIR, "ieee_template_placeholder.pdf")
    pdf.output(output_path)
    print(f"Generated {output_path}")


if __name__ == "__main__":
    generate_digital_word_export()
    generate_scanned_pdfs()
    generate_ieee_template_placeholder()
