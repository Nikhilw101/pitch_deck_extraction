#!/usr/bin/env bash
# Pitch Deck Extractor - Test Runner
# Usage: ./scripts/run_tests.sh   or   bash scripts/run_tests.sh
# Run from repo root.

set -e
cd "$(dirname "$0")/.."

echo "=== Pitch Deck Extractor - Tests ==="
echo ""

if [ -f .env ]; then
  echo "[OK] .env found (required for E2E: COHERE_API_KEY)"
else
  echo "[WARN] No .env - copy .env.example and set COHERE_API_KEY for full E2E"
fi

echo ""
echo "1. Unit tests (lib + utils)..."
cargo test --lib
echo "[OK] Unit tests passed"

echo ""
echo "2. Integration tests..."
cargo test --test integration_tests
echo "[OK] Integration tests passed"

echo ""
echo "3. Smoke test (upload)..."
cargo test --test smoke_test_upload
echo "[OK] Smoke upload tests passed"

echo ""
echo "4. E2E backend flow (upload + search)..."
cargo test --test smoke_e2e_backend || {
  echo "[INFO] E2E may fail without COHERE_API_KEY and tests/test_pdf.pdf"
  exit 1
}
echo "[OK] E2E tests passed"

echo ""
echo "=== All tests passed ==="
