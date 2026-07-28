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

if __name__ == "__main__":
    generate_digital_word_export()
    generate_scanned_pdfs()
