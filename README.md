# Pitch Deck Extractor

Production-ready monorepo for extracting, structuring, validating, and reporting insights from PDF/PPT pitch decks.

## What This Project Does

- Accepts deck uploads (`.pdf`, `.pptx`) through an API.
- Extracts slide content (text, bullets, tables, images/OCR).
- Groups slides into business sections.
- Produces structured section data, signals, red flags, confidence scoring, and summaries.
- Exposes a React UI for upload, report review, and PDF export.

## Repository Structure

```text
pitch_deck_extractor/
  backend/                      Rust API and processing pipeline
    src/
      controllers/              API handlers
      models/                   Domain models and response schemas
      services/                 Extraction, LLM, scoring, validation, web checks
      utils/                    Shared helpers
    scripts/                    Supporting scripts (classification, extraction)
    tests/                      Integration and smoke tests
    Cargo.toml
    .env.example
    README.md

  frontend/
    pitch-deck-ui/              React + Vite application
      src/
        features/pitch-deck/    Upload flow, report rendering, PDF export
        components/             Shared UI components
      package.json
      README.md

  docs/                         Architecture and project documentation
  outputs/                      Local generated outputs (ignored)
  README.md
```

## Technology Stack

- Backend: Rust, Tokio, Axum, MongoDB driver, Cohere embeddings, Ollama, HNSW (`hnsw_rs`)
- Frontend: React, Vite, Tailwind, MUI, Radix UI

## Quick Start

### 1) Start Backend

```bash
cd backend
cp .env.example .env
cargo run
```

Backend default URL: `http://127.0.0.1:3000`

### 2) Start Frontend

```bash
cd frontend/pitch-deck-ui
npm install
npm run dev
```

Frontend default URL: `http://localhost:5173`

## Core API Endpoints

- `GET /api/health` - service status
- `POST /api/decks/upload` - upload and process deck
- `GET /api/jobs/status/:job_id` - processing status
- `POST /api/decks/search` - semantic deck search

## Recent Improvements

- Red flag output now includes:
  - proof/evidence text
  - evidence confirmation status
  - source engine label
  - human-readable reason details
- Red flag de-duplication improved across section-level and overall output.
- Frontend and PDF report now display red flag origin and reason context.
- Repository cleaned from generated/local artifacts and temporary test files.

## Development Standards

- Keep secrets in local `.env` only.
- Do not commit generated runtime artifacts.
- Keep backend/frontend concerns separated by directory.
- Keep long-form architecture/process notes under `docs/`.
