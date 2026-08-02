# ⚡ LightningParse

Fast, accurate PDF parsing for RAG pipelines — a Rust extraction core (via PyO3) feeding a Python chunking/embedding/retrieval pipeline.

> **Status:** core pipeline complete — Rust extraction, cleanup, OCR fallback, chunking, retrieval, and generation are all implemented and benchmarked end-to-end. See [`PHASES.md`](./PHASES.md) for what's built and [`BENCHMARKS.md`](./benchmarks/BENCHMARKS.md) for full results.

## What's New in v0.2.0

- **Structured table extraction**: tables with detected captions are now parsed into structured row/column data instead of flat text, with markdown-formatted output in RAG chunks
- **CID/Type0 composite font support**: proper `/W` and `/DW` array parsing for embedded CJK and other composite fonts (previously fell back to a fixed 0.5em width)
- **New robustness fixtures**: added synthetic distorted-scan and Word-export test cases to broaden Tier 2/Tier 1 coverage
- **Fixed**: a performance regression introduced during table-detection development (`O(N²)` → `O(N)`)
- **Fixed**: a false-positive table detection issue that was incorrectly merging multi-author affiliation blocks

## Why

Traditional Python PDF libraries (PyPDF2, pdfplumber, PyMuPDF) are GIL-bound and process pages sequentially, which becomes a bottleneck in RAG ingestion pipelines. LightningParse pushes extraction, header/footer cleanup, and OCR fallback into Rust, parallelized across pages, and returns structured JSON that Python can chunk with page/section metadata intact.

## Architecture

```
React → FastAPI → Rust PDF Parser (PyO3) → Chunker → FAISS/Chroma → LLM
```

Two processing tiers:
- **Tier 1 — Digital-native PDFs:** direct text extraction, no OCR. This is where the speed claim is benchmarked.
- **Tier 2 — Scanned/image PDFs:** routed per-page to OCR (Tesseract) when no text layer is present.

Full design details: [`ARCHITECTURE.md`](./ARCHITECTURE.md)
Product scope and roadmap: [`PRD.md`](./PRD.md)
Contributor/agent instructions: [`AGENTS.md`](./AGENTS.md)

## Benchmarks

LightningParse is **6.0×–93.1× faster** than pypdf/pdfplumber on digital-native (Tier 1) PDFs, with the gap widening on longer documents. Some representative results:

| Document | Pages | LightningParse (median) | pypdf | pdfplumber |
|---|---:|---:|---:|---:|
| Multi-page IEEE paper (`ieee_template_placeholder.pdf`) | 8 | 0.61 ms | 7.89 ms (12.9× slower) | 56.82 ms (93.1× slower) |
| Two-column academic paper (`arxiv_twocolumn.pdf`) | 15 | 41.12 ms | 951.92 ms (23.1× slower) | 2579.90 ms (62.7× slower) |
| Single-page resume (`Shivam_FullStack.pdf`) | 1 | 6.82 ms | 82.14 ms (12.0× slower) | 208.42 ms (30.6× slower) |

OCR (Tier 2) and mixed-document handling are also supported, benchmarked separately from Tier 1 — pypdf and pdfplumber can't perform OCR, so comparing their near-instant-but-empty results against LightningParse's actual OCR time would be misleading rather than informative. See `BENCHMARKS.md` for those numbers on their own terms.

A concurrent-load test also confirms the Rust FFI genuinely releases Python's GIL during parsing: 10 concurrent OCR-heavy parse requests complete **4.78× faster** than running them sequentially, on an 8-core/16-thread machine.

Full methodology, per-document results, and reproduction steps: [`benchmarks/BENCHMARKS.md`](./benchmarks/BENCHMARKS.md). Run them yourself:

```bash
cd benchmarks
python benchmark.py --tier all
```

Results are published in `benchmarks/BENCHMARKS.md` — generated, not hand-written.

## Known Limitations

- **CID/Type0 composite fonts:** glyph width lookup currently only reads `/Widths` (simple fonts); CID fonts fall back to a standard 0.5 em width, verified safe (no crash) but not pixel-precise for bbox positioning. See `ARCHITECTURE.md` decision log.
- **Unsupported content stream filters (ASCII85Decode, etc.):** lopdf 0.33 only decodes `FlateDecode` and `LZWDecode` content stream filters. PDFs using other filters (most commonly `ASCII85Decode`, found in older PDF generators and reportlab output) will silently produce zero text blocks from Tier 1 extraction. These pages get misrouted to Tier 2 OCR, producing degraded output — OCR on a digital-native PDF loses font metadata, introduces character errors, and is much slower than the digital extraction that should have happened. This is a correctness issue affecting a minority of real-world PDFs, not just synthetic fixtures. A fix (adding a ~40-line ASCII85 decoder) is straightforward and tracked for a future phase.
- **OCR noise:** Tesseract confidence-based filtering removes most scan artifacts (binder shadows, margin smudges) but some low-level noise can still pass through on real-world scans. OCR output is not expected to be flawless — see `PRD.md` non-goals.
- **Tier 2/Mixed fixture coverage:** currently validated against a small number of real scanned/mixed fixtures rather than a broad corpus. On the synthetic `phone_photo_invoice.pdf` fixture specifically, heavy combined distortion (rotation + noise + lighting gradient + blur) caused the OCR confidence filter to discard all real content along with the noise — 0 of 7 real lines recovered. This demonstrates the system fails safely (no crash, no hallucinated garbage) under severe distortion, but does not currently recover text from heavily degraded scans. Real-world phone photos are often less distorted than this synthetic worst-case, but this is a genuine, unresolved limitation, not just a synthetic-vs-real fidelity gap. Speedup claims for Tier 1 are well-validated across multiple document types; Tier 2 performance numbers should be read as representative of the current fixtures, not a broad guarantee.
- **Tables and complex layouts:** table extraction is flattened to text, not structured (rows/columns), in v1. Full table structure extraction is out of scope for now — see `PRD.md`.
- **Encrypted/form PDFs:** not explicitly supported in v1.

## Install

```bash
# Rust core (requires maturin)
cd lightningparse-core
maturin develop --release

# Python API
cd lightningparse-api
pip install -e .
```

## Quickstart

```python
from lightningparse import parse_pdf

result = parse_pdf("document.pdf")
for page in result["pages"]:
    for block in page["blocks"]:
        print(block["section_id"], block["text"][:80])
```

## Scope (v1)

**In scope:** digital-native PDF extraction, header/footer/footnote removal, OCR fallback for scanned pages, metadata-aware chunking, retrieval + LLM Q&A pipeline with citations.

**Not in scope yet:** structured table extraction, encrypted/form PDFs, ML-based layout detection. See `PRD.md` §2 for the full non-goals list — these are deliberate cuts, not oversights.

## Contributing

See [`AGENTS.md`](./AGENTS.md) for repo conventions, build commands, and non-negotiable rules (FFI safety, GIL handling, benchmark discipline) before opening a PR.

## License

MIT License.
