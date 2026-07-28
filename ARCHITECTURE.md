# LightningParse — Architecture

This document describes the technical design of the system: module boundaries, data flow, the FFI contract between Rust and Python, and the reasoning behind key decisions. See `PRD.md` for goals/scope and `BENCHMARKS.md` for performance data.

---

## 1. System Diagram

```
┌─────────────┐
│   React     │  (demo/test UI — not core product)
└──────┬──────┘
       │ HTTP
┌──────▼──────────────────────────────────────────┐
│  FastAPI (Python)                                │
│  - async request handling                        │
│  - calls into Rust via PyO3 (GIL released)        │
└──────┬────────────────────────────────────────────┘
       │
┌──────▼──────────────────────────────────────────┐
│  Rust Core (lightningparse-core)                  │
│                                                    │
│  ┌──────────────┐   ┌───────────────────────┐   │
│  │ Tier 1        │   │ Tier 2                 │   │
│  │ Digital-native│   │ Scanned → OCR fallback │   │
│  │ text extraction│  │ (Tesseract bindings)   │   │
│  └──────┬────────┘   └───────────┬────────────┘   │
│         │  page-level rayon parallelism            │
│         ▼                        ▼                 │
│  ┌─────────────────────────────────────────────┐  │
│  │ Header/Footer Detector (cross-page heuristic) │  │
│  └──────────────────┬──────────────────────────┘  │
│                      ▼                              │
│  ┌─────────────────────────────────────────────┐  │
│  │ Structured Output Builder → JSON              │  │
│  └─────────────────────────────────────────────┘  │
└──────┬────────────────────────────────────────────┘
       │ JSON (text, page_num, bbox, section_id)
┌──────▼──────────────────────────────────────────┐
│  Python: Metadata-aware Chunker                   │
└──────┬────────────────────────────────────────────┘
       │
┌──────▼──────────────────────────────────────────┐
│  Embeddings → FAISS / Chroma                      │
└──────┬────────────────────────────────────────────┘
       │
┌──────▼──────────────────────────────────────────┐
│  LLM (via LangChain)                              │
└────────────────────────────────────────────────────┘
```

---

## 2. Module Boundaries

### 2.1 Rust core (`lightningparse-core`)
Owns everything up through "clean structured text out." Nothing here knows about embeddings, chunking, or LLMs — that separation is deliberate so the Rust core stays independently useful (and independently benchmarkable) as a standalone library.

Sub-modules:
- `extract/` — Tier 1 digital-native extraction (per-page, parallelized)
- `ocr/` — Tier 2 scanned-page detection + Tesseract invocation
- `cleanup/` — header/footer detection, reading-order reconstruction, OCR artifact cleanup
- `output/` — structured JSON serialization
- `ffi/` — PyO3 bindings, the only module allowed to touch Python types

**Rule:** business logic (extraction, cleanup) never directly depends on PyO3 types. Only `ffi/` translates between Rust-native structs and Python. This keeps the core testable in pure Rust (`cargo test`) without spinning up Python at all.

### 2.2 Python service (`lightningparse-api`)
- `api/` — FastAPI routes, request/response models
- `chunking/` — metadata-aware chunker consuming the Rust JSON output
- `pipeline/` — embeddings, vector store (FAISS/Chroma), LangChain wiring
- `bindings.py` — thin wrapper around the PyO3 module; this is the only place that imports the compiled Rust extension directly

### 2.3 Benchmarking (`benchmarks/`)
Lives outside both — treated as a peer, not a subdirectory of either. Runs both the Rust core (via bindings) and baseline libraries (pdfplumber, PyPDF2, PyMuPDF) against the same corpus. See `BENCHMARKS.md`.

---

## 3. The FFI Contract (Rust ↔ Python)

This is the most failure-prone part of the system, so it gets explicit rules.

### 3.1 Data format across the boundary
**Decision: JSON string, not native PyO3 objects, for v1.**
Rationale: easier to version, easier to debug (can log/inspect the raw payload), and the serialization cost is negligible next to PDF parsing time itself. Revisit only if profiling shows serialization is a measurable bottleneck — don't optimize this preemptively.

Output schema (per document):
```json
{
  "pages": [
    {
      "page_num": 1,
      "blocks": [
        {
          "text": "...",
          "bbox": [x0, y0, x1, y1],
          "section_id": "header|body|footer|footnote",
          "source": "digital|ocr"
        }
      ]
    }
  ],
  "metadata": {
    "tier": "digital|scanned|mixed",
    "page_count": 12,
    "parse_time_ms": 340
  }
}
```

### 3.2 GIL handling
Rust-side parsing runs inside `Python::allow_threads`, releasing the GIL for the duration of the parse. This is non-negotiable — without it, FastAPI's async event loop stalls on every parse call regardless of how fast Rust is internally.

### 3.3 Error handling
- Rust functions return `Result<T, ParseError>` internally — never panic on malformed input.
- `catch_unwind` wraps the outermost FFI entry point as a last-resort safety net, but should rarely trigger if `Result` handling is done correctly upstream.
- `ParseError` variants map to specific Python exceptions (e.g., `CorruptPdfError`, `UnsupportedPdfError`, `OcrEngineError`) rather than a single generic exception — callers need to distinguish "this PDF is broken" from "OCR isn't installed."

### 3.4 Concurrency model
Two levels of parallelism, kept distinct:
- **Across requests:** FastAPI's async handling + GIL release (many PDFs processed concurrently)
- **Within a request:** rayon parallelizes across pages of a single PDF

These compose, but should be benchmarked both independently and together — a regression in one can hide in aggregate numbers.

---

## 4. Header/Footer Detection Design

Cross-page, not per-page. Algorithm sketch:
1. Extract all text blocks with bounding boxes for every page.
2. Bucket blocks by normalized y-position (top/bottom margin bands).
3. Within each band, cluster by text similarity across pages (allowing for page-number substitution, e.g. "Page 3 of 20" vs "Page 4 of 20").
4. Blocks appearing in the same band on ≥ N% of pages (configurable threshold, default ~70%) are flagged as header/footer and excluded from `body` section_id, tagged separately instead of deleted — so the option to include them later isn't lost.

This is heuristic, not ML-based, in v1 — intentional per PRD non-goals. Document this clearly so accuracy expectations are calibrated correctly.

---

## 5. Tier Routing Logic

Per-page decision, not per-document — a single PDF can be mixed:
```
for each page:
    if page has extractable text layer (non-trivial character count):
        route to Tier 1 (digital extraction)
    else:
        route to Tier 2 (OCR)
```
`metadata.tier` on the document is set to `"mixed"` if pages were routed differently. This matters for benchmarking — mixed documents must be reported as their own category, not blended into pure Tier 1 or Tier 2 numbers (per PRD §5.2).

---

## 6. Chunking (Python side)

Consumes the structured JSON, not raw text. Chunker is metadata-aware:
- Respects `section_id` — never splits a body paragraph across a header boundary
- Carries `page_num` forward into chunk metadata for citation/traceability in the final LLM answer
- Default strategy: semantic/paragraph-boundary chunking with page metadata attached; fixed-size character chunking is a fallback option, not the default

---

## 7. Key Design Decisions & Rationale (running log)

| Decision | Rationale | Revisit if... |
|---|---|---|
| JSON string across FFI, not native PyO3 objects | Simpler, debuggable, cost is negligible vs. parse time | Profiling shows serialization >5% of total time |
| Per-page tier routing (not per-document) | Real-world PDFs are often mixed | Never — this is core to correctness |
| Heuristic header/footer detection (not ML) | Matches v1 scope, avoids model dependency | Accuracy benchmark shows heuristic ceiling is too low |
| rayon for page-level parallelism | Simplest parallelism model for embarrassingly parallel per-page work | Page count is usually 1 (parallelism overhead not worth it) |
| OCR confidence filtering (threshold < 40) | Principled alternative to geometric heuristics. Accepts some margin noise (e.g. "S S") per v1 scope. | Noise materially degrades downstream chunking/retrieval (needs page-cropping/layout ML) |
| Tesseract for OCR in v1 | Pragmatic, well-supported Rust bindings exist | M5 accuracy benchmark shows it's insufficient |
| Simple Font /Widths extraction only (fallback 0.5 em) | CID/Type0 /W arrays are complex; standard PDF heuristic (0.5 em) is safe for missing metrics | A document with a CID font shows visibly incorrect bbox/reading-order behavior |

---

## 8. What Lives Where (quick reference)

| Concern | Location |
|---|---|
| PDF byte parsing | `lightningparse-core/src/extract/` |
| OCR invocation | `lightningparse-core/src/ocr/` |
| Header/footer logic | `lightningparse-core/src/cleanup/` |
| PyO3 bindings | `lightningparse-core/src/ffi/` |
| FastAPI routes | `lightningparse-api/api/` |
| Chunking | `lightningparse-api/chunking/` |
| Embeddings/vector store | `lightningparse-api/pipeline/` |
| Benchmark scripts + corpus | `benchmarks/` |
