# Recent Changes

## Repository Cleanup

- Removed accidental root-level Rust manifest files not used by the monorepo runtime.
- Removed generated local vector index artifacts from project root.
- Removed temporary sample test documents under backend test assets.
- Kept ignore rules aligned to prevent generated/runtime artifacts from being committed.

## Red Flag Quality Improvements

- Added red flag metadata fields:
  - `source`
  - `reason_details`
  - `evidence_text`
  - `evidence_confirmed`
  - `evidence_slide_number` (when available)
- Improved red flag post-processing:
  - placeholder cleanup (`null`, empty text, generic placeholders)
  - stronger fallback proof generation
  - section and global de-duplication improvements
  - noise filtering for non-informative flags
- Added clearer source labels in UI/PDF:
  - `LLM Structuring`
  - `Signal Extractor`
  - `Rule Engine`
  - `Derived Analysis`

## Documentation Refresh

- Rewrote root `README.md` with standard structure, setup, architecture overview, and operational notes.
- Rewrote `backend/README.md` with responsibilities, module layout, env vars, and run/test instructions.
- Rewrote `frontend/pitch-deck-ui/README.md` from template content to project-specific usage and integration notes.
