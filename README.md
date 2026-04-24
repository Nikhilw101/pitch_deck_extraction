# Pitch Deck Extractor

Production-style monorepo with a separated Rust backend and React frontend for pitch deck ingestion, analysis, and reporting.

## Repository Structure

```text
pitch_deck_extractor/
  backend/                 # Rust API + pipeline services
    src/
    scripts/
    tests/
    Cargo.toml
    .env.example
  frontend/
    pitch-deck-ui/         # React + Vite application
  docs/                    # Project documentation
  outputs/                 # Local generated artifacts (ignored in git)
```

## Tech Stack

- Backend: Rust, Tokio, Axum, MongoDB driver, Cohere embeddings, Ollama, HNSW (`hnsw_rs`)
- Frontend: React, Vite, Tailwind, MUI

## Local Development

### 1) Backend

```bash
cd backend
cp .env.example .env
cargo run
```

### 2) Frontend

```bash
cd frontend/pitch-deck-ui
npm install
npm run dev
```

## Core API Endpoints

- `GET /api/health`
- `POST /api/decks/upload`
- `GET /api/jobs/status/:job_id`
- `POST /api/decks/search`

## Production Git Standards in this Repo

- Backend and frontend are separated by directory
- Generated artifacts, local secrets, and build outputs are ignored
- No IDE/vendor metadata is tracked (`.cursor`, `.vscode`, `.idea`)

## Notes

- Keep secrets only in local `.env` files (never commit)
- Keep architecture and API details in `docs/` for long-form documentation
