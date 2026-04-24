# Frontend Integration Guide (React.js)

This document is a frontend-only integration guide for connecting a React.js app to the current backend (`pitch_deck_extractor`) **without changing backend code**.

---

## 1) Backend Base Details

- Default backend base URL: `http://127.0.0.1:3000`
- Content type used by API: `application/json` (except file upload: `multipart/form-data`)
- CORS is enabled via backend env `FRONTEND_ORIGIN` (must match your frontend URL)

### Required backend env for frontend integration

In backend `.env`:

- `PORT=3000` (or your preferred port)
- `FRONTEND_ORIGIN=http://localhost:5173` (Vite React default)
- `COHERE_API_KEY=...` (required for embeddings/search)
- `OLLAMA_BASE_URL=http://localhost:11434` (if using local Ollama)
- `OLLAMA_MODEL=...` (configured model)

---

## 2) API Endpoints for Frontend

### A) Health Check

- Method: `GET`
- URL: `/api/health`
- Purpose: Check backend availability before showing upload/search UI

Success example:

```json
{
  "status": "success",
  "message": "Service is healthy",
  "data": {
    "service": "pitch_deck_extractor",
    "status": "ok"
  },
  "request_id": "system-health-check",
  "timestamp": "2026-03-31T16:01:30.394137600+00:00"
}
```

---

### B) Upload Deck (PDF/PPTX)

- Method: `POST`
- URL: `/api/decks/upload`
- Content type: `multipart/form-data`
- Field name: `file`
- Allowed file extensions: `.pdf`, `.pptx`

Request example (frontend behavior):

- Create `FormData`
- `formData.append("file", selectedFile)`
- `fetch('/api/decks/upload', { method: 'POST', body: formData })`

Success response envelope:

```json
{
  "status": "success",
  "message": "Deck processed successfully",
  "data": {
    "deck_id": "uuid",
    "filename": "test_pdf.pdf",
    "file_type": "pdf",
    "total_slides": 18,
    "metadata": {
      "extraction_timestamp": "2026-03-31T16:02:30.167910500+00:00",
      "extraction_method": "unified_4_stage_pipeline",
      "has_speaker_notes": false,
      "has_hidden_slides": false,
      "has_tables": true,
      "has_charts": false
    },
    "indexing": {
      "slides_indexed": 18,
      "embedding_dimension": 1024,
      "status": "indexed"
    },
    "grouped_deck": {},
    "structured_output": {}
  },
  "request_id": "uuid",
  "timestamp": "2026-03-31T16:14:56.637209300+00:00"
}
```

Error response envelope:

```json
{
  "status": "error",
  "error": {
    "code": "UNSUPPORTED_FILE_TYPE",
    "message": "Unsupported file type. Only PDF and PPTX are allowed. Received: file.exe"
  },
  "request_id": "uuid",
  "timestamp": "2026-03-31T16:10:00Z"
}
```

Common error codes for upload:

- `UNSUPPORTED_FILE_TYPE` (400)
- `INVALID_MULTIPART` (400)
- `FILE_PROCESSING_ERROR` (422)
- `EXTRACTION_ERROR` (422)
- `INTERNAL_ERROR` (500)

---

### C) Semantic Search

- Method: `POST`
- URL: `/api/decks/search`
- Content type: `application/json`

Request body:

```json
{
  "query": "revenue growth",
  "limit": 5
}
```

Validation rules:

- `query` must not be empty
- `limit` must be between `1` and `20`

Success response envelope:

```json
{
  "status": "success",
  "message": "Search completed successfully",
  "data": {
    "results": [
      {
        "deck_id": "uuid",
        "slide_number": 5,
        "score": 0.21,
        "text_snippet": "Revenue grew 120% YoY..."
      }
    ]
  },
  "request_id": "uuid",
  "timestamp": "2026-03-31T16:20:00Z"
}
```

Validation error example:

```json
{
  "status": "error",
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "limit must be between 1 and 20"
  },
  "request_id": "uuid",
  "timestamp": "2026-03-31T16:20:05Z"
}
```

---

## 3) React Frontend Requirements (Implementation Checklist)

## Core API setup

- Define single API base URL via env:
  - Vite: `VITE_API_BASE_URL=http://127.0.0.1:3000`
- Create centralized API client module for:
  - `healthCheck()`
  - `uploadDeck(file)`
  - `searchDecks(query, limit)`
- Always parse and handle backend envelope (`status`, `data`/`error`, `request_id`)

## Upload UI requirements

- File input with extension filter: `.pdf,.pptx`
- Validate before sending:
  - file selected
  - extension allowed
  - optional size warning for large files
- Show request lifecycle states:
  - `idle`
  - `uploading/processing` (long-running)
  - `success`
  - `error`
- On success, store:
  - `deck_id`
  - `indexing.status`
  - `structured_output` (if present)
- On error, show:
  - `error.message`
  - `request_id` for support/debug

## Search UI requirements

- Query input (non-empty)
- Limit input/select constrained to `1..20`
- Disable submit while loading
- Render results table/list:
  - `slide_number`
  - `score`
  - `text_snippet`
- If `results` empty, show "No matches found"

## Health + startup behavior

- Call `/api/health` on app startup
- If backend unavailable:
  - show offline banner
  - disable upload/search actions

---

## 4) Recommended Frontend Data Models (TypeScript)

Use these interfaces in frontend for strict typing:

```ts
export interface ApiSuccess<T> {
  status: "success";
  message: string;
  data: T;
  request_id: string;
  timestamp: string;
}

export interface ApiError {
  status: "error";
  error: {
    code: string;
    message: string;
  };
  request_id: string;
  timestamp: string;
}

export type ApiEnvelope<T> = ApiSuccess<T> | ApiError;

export interface UploadResponseData {
  deck_id: string;
  filename: string;
  file_type: "pdf" | "pptx";
  total_slides: number;
  metadata: {
    extraction_timestamp: string;
    extraction_method: string;
    has_speaker_notes: boolean;
    has_hidden_slides: boolean;
    has_tables: boolean;
    has_charts: boolean;
  };
  indexing: {
    slides_indexed: number;
    embedding_dimension: number;
    status: string;
  };
  grouped_deck?: unknown;
  structured_output?: unknown;
}

export interface SearchRequest {
  query: string;
  limit: number;
}

export interface SearchResultItem {
  deck_id: string;
  slide_number: number;
  score: number;
  text_snippet?: string;
}

export interface SearchResponseData {
  results: SearchResultItem[];
}
```

---

## 5) UX Behavior for Long-Running Upload

Upload endpoint can take significant time (multi-stage extraction + embeddings + LLM processing). Frontend must:

- Keep spinner/progress indicator visible while request is active
- Show processing message such as:
  - "Analyzing deck, this can take a few minutes..."
- Set request timeout in frontend client high enough (or no client timeout if acceptable)
- Provide cancel/retry button in UI
- Handle network interruption gracefully

---

## 6) Error Handling Strategy (Frontend)

Always branch on envelope:

- If `status === "success"`:
  - consume `data`
- If `status === "error"`:
  - show `error.message`
  - log/display `request_id`

Also handle non-envelope failures:

- network error
- CORS blocked error
- non-JSON response (fallback generic error message)

Recommended user-safe message fallback:

- "Something went wrong while processing your request. Please try again."

---

## 7) CORS Troubleshooting

If frontend gets CORS error:

1. Confirm backend `.env` has exact frontend origin:
   - `FRONTEND_ORIGIN=http://localhost:5173` (or your frontend URL)
2. Restart backend after `.env` change
3. Confirm frontend calls correct backend URL and protocol
4. Check browser console for blocked origin details

---

## 8) Frontend Test Checklist (Manual)

- Health endpoint reachable from frontend startup
- PDF upload succeeds and returns `deck_id`
- Unsupported file type returns structured error
- Search with valid query/limit returns success
- Search with empty query returns validation error
- Loading and error states render correctly
- `request_id` shown in error UI/logs

---

## 9) Example Frontend Env (`.env` in React app)

For Vite:

```env
VITE_API_BASE_URL=http://127.0.0.1:3000
```

For CRA:

```env
REACT_APP_API_BASE_URL=http://127.0.0.1:3000
```

---

## 10) Integration Summary

Frontend integration is ready with current backend contract. React app should treat upload as long-running, parse standardized envelopes, and implement strong error/validation handling around the three endpoints:

- `GET /api/health`
- `POST /api/decks/upload`
- `POST /api/decks/search`

This file documents all required frontend behavior without modifying backend code.
