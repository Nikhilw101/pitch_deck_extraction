# Backend Service

Rust backend for pitch deck upload, extraction, indexing, and analysis.

## Run

```bash
cp .env.example .env
cargo run
```

## Test

```bash
cargo test
```

## Main Responsibilities

- Deck ingestion and parsing
- Embedding generation and vector indexing
- LLM-based section structuring and scoring
- Job orchestration and status APIs
