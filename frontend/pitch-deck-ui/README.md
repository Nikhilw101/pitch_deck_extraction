# Pitch Deck UI (React + Vite)

Frontend application for uploading decks and viewing structured analysis results.

## What This UI Includes

- Backend health status panel
- Deck upload workflow
- Full section-wise structured report view
- Red flag proof, reason, source, and confirmation rendering
- PDF report export
- Raw JSON inspection tab

## Folder Structure

```text
frontend/pitch-deck-ui/
  src/
    components/                       shared UI primitives
    features/pitch-deck/
      components/                     upload/report UI
      lib/                            report/PDF helpers
      hooks/                          feature hooks
    App.jsx
  public/
  package.json
```

## Prerequisites

- Node.js 18+
- Backend service running at `http://127.0.0.1:3000` (or configured URL)

## Run Locally

```bash
npm install
npm run dev
```

Default app URL: `http://localhost:5173`

## Available Scripts

- `npm run dev` - start development server
- `npm run build` - create production build
- `npm run preview` - preview production build locally
- `npm run lint` - run ESLint

## Backend Integration

- The UI expects the backend response envelope:
  - `status`
  - `message`
  - `data`
  - `request_id`
  - `timestamp`
- Upload endpoint used: `POST /api/decks/upload`
- Search endpoint used: `POST /api/decks/search`
- Health endpoint used: `GET /api/health`

For full API integration details, see `FRONTEND_INTEGRATION_README.md` at repository root.
