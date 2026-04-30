# Backend Service

Rust backend for ingestion, extraction, indexing, structuring, scoring, and reporting.

## Responsibilities

- File ingestion and deck parsing (`pdf`, `pptx`)
- Slide element extraction (text, bullets, stats, tables, OCR)
- Section classification and grouping
- LLM-based structured extraction and summarization
- Signal and red-flag generation with proof metadata
- Multi-factor confidence scoring
- Optional web consistency validation
- Search indexing and semantic retrieval

## Project Layout

```text
backend/
  src/
    controllers/     HTTP endpoints
    models/          API and pipeline data models
    services/        Pipeline services (LLM, scoring, validation, search, web)
    utils/           Utility helpers
    main.rs          App entrypoint
    lib.rs           Shared library entrypoint
  scripts/           Offline helpers for extraction/classification
  tests/             Integration/smoke tests
  Cargo.toml
  .env.example
```

## Run Locally

```bash
cp .env.example .env
cargo run
```

Default server URL: `http://127.0.0.1:3000`

## Test

```bash
cargo test
```

## Key Environment Variables

- `PORT` - API server port (default `3000`)
- `FRONTEND_ORIGIN` - allowed CORS origin for UI
- `COHERE_API_KEY` - embeddings provider key
- `OLLAMA_BASE_URL` - local/model inference base URL
- `OLLAMA_MODEL` - extraction model name
- `INDEX_PATH` - local vector index path prefix

See `.env.example` for full list.

## API Surface

- `GET /api/health`
- `POST /api/decks/upload`
- `GET /api/jobs/status/:job_id`
- `POST /api/decks/search`
