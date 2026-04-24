# Line-by-Line Logic Review and Fixes

## Summary

The codebase was reviewed line-by-line for correctness. **Several logic and structure issues were found and fixed.** Build and all tests pass.

---

## 1. `src/services/extraction_service.rs`

### 1.1 Blocking call in async context (fixed)
- **Issue:** `Command::new("pdftotext").output()` is a blocking system call. Calling it inside an `async fn` blocks the async runtime and can stall the server.
- **Fix:** Run `pdftotext` inside `tokio::task::spawn_blocking()` so it runs in a thread pool. Handle both `JoinError` (from spawn) and `io::Error` (from `output()`) and map them to `AppError`.

### 1.2 PDF slide numbering after filtering empty pages (fixed)
- **Issue:** `parse_pdf_text_enhanced` used `enumerate()` after `filter()`, so `slide_number` was `idx + 1` where `idx` was the index in the **original** page list. Empty pages were skipped but indices were not re-based, so slide numbers could be non-consecutive (e.g. 2, 3, 5 if pages 1 and 4 were empty).
- **Fix:** Iterate over pages with an explicit `slide_number` counter that increments only for non-empty pages. Slide numbers are now 1-based and consecutive.

### 1.3 Only first bullet list and content after it dropped (fixed)
- **Issue:** In `parse_page_structure`, after finding the first bullet line the code called `parse_bullet_list` then **broke** out of the loop. So only one bullet list per page was kept, and any text or bullet lists after it were ignored.
- **Fix:** Replaced the `for` loop with a `while` loop and an index `i`. When bullets are found, `parse_bullet_list` now returns `(Vec<BulletItem>, next_index)`. We push the bullet list, set `i = next_i`, and continue so multiple text blocks and bullet lists are all collected.

### 1.4 Bullet list line consumption (fixed)
- **Issue:** `parse_bullet_list` only returned `Vec<BulletItem>`, so the caller could not know how many lines were consumed (empty lines between bullets meant line count ≠ item count).
- **Fix:** `parse_bullet_list` now returns `(Vec<BulletItem>, usize)` where the `usize` is the index of the line **after** the last consumed line. Caller sets `i = next_i` so parsing continues correctly.

### 1.5 Subtitle set to a bullet line (fixed)
- **Issue:** The second non-empty line was always treated as subtitle. If that line was a bullet (e.g. "• Point 1"), it was stored as subtitle instead of as content.
- **Fix:** Subtitle is set only when the second line does **not** start with `•`, `-`, or `*`. Otherwise it is treated as content.

### 1.6 Lopdf slide numbers when some pages are empty (fixed)
- **Issue:** In `extract_pdf_with_lopdf`, when a page had no text we `continue` without pushing a slide, but `slide_number` was still `page_num`. So we could get slides with numbers 1, 3, 5 (skipping 2 and 4).
- **Fix:** Use a separate `slide_number` that increments only when we push a slide. Slide numbers are 1-based and consecutive.

### 1.7 PPTX slide number 0 and notes lookup (fixed)
- **Issue:** If the slide filename did not parse (e.g. malformed name), `slide_num` was 0. We then built a slide with `slide_number: 0`. Multiple such files could produce duplicate slide numbers, and `slide_notes.get(&0)` might not match OOXML numbering.
- **Fix:** If `slide_num == 0`, use `slide_number = slides.len() + 1` so we get a unique 1-based number. Use the same `slide_number` for both the slide and for `slide_notes.get(&slide_number)` so notes alignment stays correct when `slide_num` is valid.

---

## 2. `src/utils/validation.rs`

### 2.1 Table with zero columns (fixed)
- **Issue:** `validate_table` only checked `rows.is_empty()`. When `num_columns == 0`, every row has `total_cols == 0` (sum of colspans), so `all(|row| total_cols == expected_cols)` was true and the table was considered valid.
- **Fix:** Return `false` when `table.num_columns == 0`.

---

## 3. `src/main.rs`

### 3.1 Port config ignored (fixed)
- **Issue:** Config loads `server_port` from env (and `PORT`), but the server used `std::env::var("PORT").unwrap_or_else(...)` again instead of `config.server_port`, so config was not the single source of truth.
- **Fix:** Use `config.server_port` when binding the listener.

---

## 4. Logic verified as correct (no change)

- **deck_controller:** Multipart → one field, filename, file type, temp file, extract by type, return JSON. Temp file is dropped after use. Correct.
- **ingestion_service:** Temp file creation and write; no extension needed for extraction. Correct.
- **regex_utils:** Money/percentage/number extraction and tests. Correct.
- **validation:** Chart (same-length series), position (positive width/height), has_meaningful_content. Correct.
- **errors:** AppError variants and IntoResponse. Correct.
- **models:** Deck and content block types. Correct.
- **routes:** Single upload route and body limit. Correct.
- **config:** Load from env. Correct.
- **db:** Connect and repository. Correct (repository not yet used in flow).

---

## Tests

- All existing unit tests (validation, regex_utils) pass.
- Integration test `test_server_starts` passes.
- `cargo build` and `cargo test` succeed.

---

## Files modified

| File | Changes |
|------|--------|
| `src/services/extraction_service.rs` | spawn_blocking for pdftotext, consecutive slide numbers (PDF + lopdf), parse_page_structure while-loop and multi-block handling, parse_bullet_list returns next index, subtitle only when not bullet, PPTX slide_number fallback |
| `src/utils/validation.rs` | Reject tables with `num_columns == 0` |
| `src/main.rs` | Use `config.server_port` for bind address |
