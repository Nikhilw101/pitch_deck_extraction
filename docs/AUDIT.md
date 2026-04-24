# Pitch Deck Extractor — Project Audit

**Last Updated**: March 2026  
**Purpose**: Structured audit of what has been done and what remains.

---

## 1. WHAT HAS BEEN DONE

### 1.1 Pipeline Phases (Implemented & Working)

| Phase | Description | Status | Key Files |
|-------|-------------|--------|-----------|
| **Phase 1** | Ingestion & structural extraction (PDF/PPTX) | ✅ Complete | `extraction_service`, `ingestion_service`, `preprocessing_service` |
| **Phase 2** | Embeddings (Cohere) + HNSW vector index | ✅ Complete | `embedding_service`, `vector_store_service` |
| **Phase 3** | Section classification & semantic grouping | ✅ Complete | `section_classification_service`, `section_grouping_service` |
| **Phase 4** | LLM structured extraction, validation, summaries, signals | ✅ Complete | `structuring_service`, `validation_service`, `summarization_service`, `signals_service`, `llm_service` |
| **Phase 5** | Web fetch & validation (company facts) | ✅ Complete | `web_fetch_service`, `web_validation_service` |

### 1.2 Quality-First LLM Optimizations (Recent)

| Change | Before | After |
|--------|--------|-------|
| Truncation | Section text capped (6500/4000/5500 chars) | **No truncation** — full content sent to LLM |
| Timeout | 180s per call | **300s (5 min)** per call |
| Concurrency | 3 in-flight requests | **2** for stability |
| Summary prompts | Generic "Summarize…" | **Structured-data-specific** — "Always produce a summary; do not refuse" |
| Signals prompt | Basic instructions | **Exhaustive extraction** — "Extract ALL relevant signals" |
| Structured extraction | Standard instructions | **"Extract ALL … exhaustively; do not skip or truncate"** |

### 1.3 Output Structure

| Output | Location | Content |
|--------|----------|---------|
| **Section summaries** | `structured_output.sections[].summary` | 2–3 sentence summary per section |
| **Overall summary** | `structured_output.overall_summary` | 3–5 sentence executive summary for entire deck |
| **Structured data** | `structured_output.sections[].data` | JSON key-value pairs per section |
| **Signals** | `structured_output.sections[].signals`, `overall_signals` | Investment signals per section + consolidated |
| **Red flags** | `structured_output.sections[].red_flags`, `overall_red_flags` | Risk indicators per section + consolidated |
| **Score breakdown** | `structured_output.score_breakdown` | validation_score, llm_confidence, completeness_score, etc. |

### 1.4 Tests (Implemented)

| Test | Purpose | Output JSON | Runtime |
|------|---------|-------------|---------|
| `smoke_full_pre_frontend` | Full pipeline + semantic search | `tests/smoke_test_upload_output.json`, `tests/smoke_test_search_output.json` | ~7–15 min |
| `smoke_test_upload` | Upload endpoint (valid PDF, errors) | — | ~10 sec (errors), ~7–15 min (valid PDF) |
| `smoke_phase1_extraction` | PDF extraction only | `tests/smoke_phase1_output.json` | ~2 sec |
| `smoke_phase1_comprehensive` | Phase 1 comprehensive | `tests/smoke_test_output.json` | ~3 sec |
| `smoke_phase1_pptx` | PPTX extraction | `tests/smoke_test_output_pptx.json` | ~3 sec |
| `smoke_test_e2e` | E2E report | `tests/smoke_test_results.json` | ~10 min |
| `smoke_e2e_backend` | Backend E2E | — | ~10 min |
| `integration_tests` | Placeholder | — | < 1 sec |

### 1.5 Documentation

| Doc | Content |
|-----|---------|
| `docs/LLM_PROMPTS_AND_ACCURACY.md` | Prompt types, schema hints, 10 accuracy tips, quality-first pipeline settings |
| `README.md` | Architecture, phases, running instructions |

### 1.6 API & Routes

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/decks/upload` | POST | Upload PDF/PPTX → extraction, indexing, classification, LLM processing |
| `/api/decks/search` | POST | Semantic search over indexed deck |

---

## 2. WHAT REMAINS (Optional / Future)

### 2.1 Not Implemented (Lower Priority)

| Item | Description |
|------|-------------|
| Two-step extraction | Step 1: extract facts in text; Step 2: convert to JSON — reduces hallucination |
| Self-verification pass | Post-extraction: "Review JSON vs original text; correct errors" |
| Field-level confidence | Per-field confidence (0–1) in extraction prompt |
| Truncation toggle | `truncate_for_llm()` exists but unused; could add env flag to re-enable for speed |

### 2.2 Known Limitations

| Limitation | Mitigation |
|------------|------------|
| Ollama 300s timeout can still hit on very long sections | Use stronger/faster model or re-enable truncation for edge cases |
| Section summaries sometimes "refuse" on JSON input | Prompts updated; if persists, consider dedicated `generate_summary_from_structured()` |
| Small models (3B) miss details | Use Mistral 7B / Llama 3.1 8B per docs |
| smoke_test_upload valid PDF requires full env | Needs COHERE_API_KEY + Ollama; ~7–15 min runtime |

### 2.3 Test Coverage Gaps

| Gap | Suggestion |
|-----|------------|
| No unit tests for summarization/signals prompts | Add mock LlmService for prompt validation |
| smoke_test_e2e may use old routes | Verify `/api/decks/upload` and `/api/decks/search` |
| No automated run script | Use `run_tests.ps1` or `run_tests.sh` |

---

## 3. JSON OUTPUT FILE LOCATIONS (After Tests Run)

| Test | JSON Output Path | Description |
|------|------------------|-------------|
| `smoke_full_pre_frontend` | `tests/smoke_test_upload_output.json` | Full upload/LLM response (structured_output, sections, summaries, signals) |
| `smoke_full_pre_frontend` | `tests/smoke_test_search_output.json` | Semantic search results |
| `smoke_phase1_extraction` | `tests/smoke_phase1_output.json` | Phase 1 extraction (slides, elements) |
| `smoke_phase1_comprehensive` | `tests/smoke_test_output.json` | Comprehensive Phase 1 report |
| `smoke_phase1_pptx` | `tests/smoke_test_output_pptx.json` | PPTX extraction output |
| `smoke_test_e2e` | `tests/smoke_test_results.json` | E2E test report |
| Vector index metadata | `tests/tmp_full_smoke_index.meta.json` | HNSW index metadata (full smoke) |
| Vector index metadata | `tests/tmp_smoke_upload_index.meta.json` | HNSW index metadata (upload smoke) |

---

## 4. RUNNING TESTS (Commands)

```powershell
# Quick (~5 sec)
cargo test --test integration_tests --test smoke_phase1_extraction -- --nocapture

# Upload error paths (~15 sec)
cargo test --test smoke_test_upload smoke_upload_unsupported smoke_upload_txt smoke_upload_invalid smoke_upload_corrupted -- --nocapture

# Full pipeline (~7–15 min, needs Ollama + Cohere)
cargo test --test smoke_full_pre_frontend -- --nocapture
cargo test --test smoke_test_upload smoke_upload_valid_pdf -- --nocapture

# All tests
cargo test -- --nocapture
```
