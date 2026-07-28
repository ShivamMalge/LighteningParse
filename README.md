# ⚡ LightningParse

Fast, accurate PDF parsing for RAG pipelines — a Rust extraction core (via PyO3) feeding a Python chunking/embedding/retrieval pipeline.

> **Status:** core pipeline complete — Rust extraction, cleanup, OCR fallback, chunking, retrieval, and generation are all implemented and benchmarked end-to-end. See [`PHASES.md`](./PHASES.md) for what's built and [`BENCHMARKS.md`](./benchmarks/BENCHMARKS.md) for full results.

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

LightningParse is **12.9×–240.5× faster** than pypdf/pdfplumber on digital-native (Tier 1) PDFs, with the gap widening on longer documents. Some representative results:

| Document | Pages | LightningParse (median) | pypdf | pdfplumber |
|---|---:|---:|---:|---:|
| Multi-page IEEE paper (`draft10.pdf`) | 8 | 6.04 ms | 330.73 ms (54.8× slower) | 1452.80 ms (240.5× slower) |
| Two-column academic paper (`arxiv_twocolumn.pdf`) | 15 | 39.28 ms | 1436.14 ms (36.6× slower) | 3601.25 ms (91.7× slower) |
| Single-page resume | 1 | 6.68 ms | 86.02 ms (12.9× slower) | 210.21 ms (31.5× slower) |

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
