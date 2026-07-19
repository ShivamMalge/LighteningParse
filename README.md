# ⚡ LightningParse

Fast, accurate PDF parsing for RAG pipelines — a Rust extraction core (via PyO3) feeding a Python chunking/embedding/retrieval pipeline.

> **Status:** early development. Benchmark numbers below are placeholders until M2 (see `PRD.md`) — no speed claims are final until backed by `benchmarks/BENCHMARKS.md`.

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

Reproducible, versioned benchmark corpus and results live in [`benchmarks/`](./benchmarks). Run them yourself:

```bash
cd benchmarks
python benchmark.py --all
```

Results are published in `benchmarks/BENCHMARKS.md` — generated, not hand-written. Tier 1 and Tier 2 results are reported separately; they measure different things and shouldn't be blended.

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

**In scope:** digital-native PDF extraction, header/footer removal, OCR fallback for scanned pages, metadata-aware chunking.

**Not in scope yet:** structured table extraction, encrypted/form PDFs, ML-based layout detection. See `PRD.md` §2 for the full non-goals list — these are deliberate cuts, not oversights.

## Contributing

See [`AGENTS.md`](./AGENTS.md) for repo conventions, build commands, and non-negotiable rules (FFI safety, GIL handling, benchmark discipline) before opening a PR.

## License

TBD.
