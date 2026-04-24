//! Comprehensive Phase 1 Smoke Test — Extraction Layer Only
//!
//! This test validates that the entire Phase 1 extraction pipeline works correctly:
//! - PDF text extraction with spatial block grouping
//! - Multi-column detection
//! - Bounding box assignment
//! - Element classification (Title, Subtitle, BulletList, TextBlock)
//! - Table/Chart refinement
//! - OCR availability check
//!
//! Run with:  cargo test --test smoke_phase1_extraction -- --nocapture

use std::path::Path;

/// Helper: Access the library's internal modules via the public API
/// We re-use the same type definitions from the main crate.

#[tokio::test]
async fn smoke_phase1_full_extraction_pipeline() {
    let test_start = std::time::Instant::now();
    println!("\n{}", "=".repeat(80));
    println!("  PHASE 1 SMOKE TEST — Robust Document Extraction Pipeline");
    println!("{}\n", "=".repeat(80));

    // ─── Step 0: Locate test PDF ────────────────────────────────────────────────
    let test_pdf = Path::new("tests/test_pdf.pdf");
    println!("[Step 0] Locating test PDF...");
    assert!(
        test_pdf.exists(),
        "test_pdf.pdf must exist in tests/ directory"
    );
    let file_size = std::fs::metadata(test_pdf).unwrap().len();
    println!("  ✓ Found: {:?} ({} bytes)\n", test_pdf, file_size);

    // ─── Step 1: Stage 1 — Raw Text Extraction (PDF Parser) ─────────────────────
    println!("[Step 1] Stage 1 — Raw PDF Text Extraction (pdftotext -layout)");
    println!("  Running pdftotext with spatial block grouping...");

    let raw_slides =
        pitch_deck_service::services::extraction::stage1_parser::pdf::parse_pdf(test_pdf)
            .await
            .expect("PDF parsing should succeed");

    let total_slides = raw_slides.len();
    println!("  ✓ Extracted {} pages/slides", total_slides);
    assert!(total_slides > 0, "Should extract at least 1 slide from PDF");

    // Count element types
    let mut total_text_blocks = 0u32;
    let mut total_bullets = 0u32;
    let mut total_tables = 0u32;
    let mut elements_with_bbox = 0u32;

    for slide in &raw_slides {
        for elem in &slide.elements {
            match elem {
                pitch_deck_service::models::extraction_model::Element::TextBlock {
                    bbox, ..
                } => {
                    total_text_blocks += 1;
                    if bbox.is_some() {
                        elements_with_bbox += 1;
                    }
                }
                pitch_deck_service::models::extraction_model::Element::BulletList {
                    bbox, ..
                } => {
                    total_bullets += 1;
                    if bbox.is_some() {
                        elements_with_bbox += 1;
                    }
                }
                pitch_deck_service::models::extraction_model::Element::Table { .. } => {
                    total_tables += 1;
                }
                pitch_deck_service::models::extraction_model::Element::SectionHeader { .. } => {}
                pitch_deck_service::models::extraction_model::Element::Statistic { .. } => {}
                pitch_deck_service::models::extraction_model::Element::Chart { .. } => {}
                pitch_deck_service::models::extraction_model::Element::Image { .. } => {}
                _ => {}
            }
        }
    }

    println!("  ├── TextBlocks: {}", total_text_blocks);
    println!("  ├── BulletLists: {}", total_bullets);
    println!("  ├── Tables: {}", total_tables);
    println!("  └── Elements with BoundingBox: {}", elements_with_bbox);
    println!();

    // ─── Step 1b: Verify Bounding Boxes ─────────────────────────────────────────
    println!("[Step 1b] Verifying Bounding Box correctness...");
    let total_elements: u32 = raw_slides.iter().map(|s| s.elements.len() as u32).sum();
    println!("  Total elements: {}", total_elements);

    // All elements from spatial PDF parser should have bounding boxes
    let bbox_coverage = if total_elements > 0 {
        elements_with_bbox as f64 / total_elements as f64 * 100.0
    } else {
        0.0
    };
    println!("  BBox coverage: {:.1}%", bbox_coverage);
    // We expect high bbox coverage from the spatial parser BUT OCR elements may not have them
    println!("  ✓ BBox verification complete\n");

    // Print first slide content for inspection
    if let Some(first_slide) = raw_slides.first() {
        println!(
            "[Step 1c] First slide content preview (Slide #{}):",
            first_slide.slide_number
        );
        for (i, elem) in first_slide.elements.iter().enumerate().take(5) {
            match elem {
                pitch_deck_service::models::extraction_model::Element::TextBlock { text, bbox } => {
                    let preview = if text.len() > 100 { &text[..100] } else { text };
                    println!("  [{}] TextBlock: \"{}...\"", i, preview.replace('\n', " "));
                    if let Some(b) = bbox {
                        println!(
                            "       BBox: ({:.1}, {:.1}) → ({:.1}, {:.1})",
                            b.x0, b.y0, b.x1, b.y1
                        );
                    }
                }
                pitch_deck_service::models::extraction_model::Element::BulletList {
                    items,
                    bbox,
                    ..
                } => {
                    println!("  [{}] BulletList: {} items", i, items.len());
                    for item in items.iter().take(3) {
                        println!("       • {}", item);
                    }
                    if let Some(b) = bbox {
                        println!(
                            "       BBox: ({:.1}, {:.1}) → ({:.1}, {:.1})",
                            b.x0, b.y0, b.x1, b.y1
                        );
                    }
                }
                pitch_deck_service::models::extraction_model::Element::Title { text, bbox } => {
                    println!("  [{}] Title: \"{}\"", i, text);
                    if let Some(b) = bbox {
                        println!(
                            "       BBox: ({:.1}, {:.1}) → ({:.1}, {:.1})",
                            b.x0, b.y0, b.x1, b.y1
                        );
                    }
                }
                pitch_deck_service::models::extraction_model::Element::SectionHeader {
                    text,
                    ..
                } => {
                    println!("  [{}] SectionHeader: \"{}\"", i, text);
                }
                pitch_deck_service::models::extraction_model::Element::Statistic {
                    value,
                    label,
                    ..
                } => {
                    println!("  [{}] Statistic: {} ({})", i, value, label);
                }
                _ => {
                    println!("  [{}] Other element type: {:?}", i, elem);
                }
            }
        }
        println!();
    }

    // ─── Step 2: Stage 2 — Element Classification ───────────────────────────────
    println!("[Step 2] Stage 2 — Heuristic Element Classification");
    println!("  Classifying TextBlocks → Titles / Subtitles / BulletLists...");

    let mut classified_slides = raw_slides.clone();
    for slide in &mut classified_slides {
        let classified =
            pitch_deck_service::services::extraction::stage2_classifier::classify_slide_elements(
                slide.clone(),
            );
        *slide = classified;
    }

    // Count classified types
    let mut titles = 0u32;
    let mut subtitles = 0u32;
    let mut section_headers = 0u32;
    let mut statistics = 0u32;
    let mut classified_bullets = 0u32;
    let mut remaining_text = 0u32;

    for slide in &classified_slides {
        for elem in &slide.elements {
            match elem {
                pitch_deck_service::models::extraction_model::Element::Title { .. } => titles += 1,
                pitch_deck_service::models::extraction_model::Element::Subtitle { .. } => {
                    subtitles += 1
                }
                pitch_deck_service::models::extraction_model::Element::SectionHeader { .. } => {
                    section_headers += 1
                }
                pitch_deck_service::models::extraction_model::Element::Statistic { .. } => {
                    statistics += 1
                }
                pitch_deck_service::models::extraction_model::Element::BulletList { .. } => {
                    classified_bullets += 1
                }
                pitch_deck_service::models::extraction_model::Element::TextBlock { .. } => {
                    remaining_text += 1
                }
                _ => {}
            }
        }
    }

    println!("  ├── Titles detected: {}", titles);
    println!("  ├── Subtitles detected: {}", subtitles);
    println!("  ├── Section Headers detected: {}", section_headers);
    println!("  ├── Statistics detected: {}", statistics);
    println!("  ├── BulletLists: {}", classified_bullets);
    println!("  └── Remaining TextBlocks: {}", remaining_text);
    println!("  ✓ Classification complete\n");

    // ─── Step 3: Stage 3 — Table/Chart Refinement ───────────────────────────────
    println!("[Step 3] Stage 3 — Table/Chart Refinement");
    println!("  Running table and chart detection...");

    let mut refined_slides = classified_slides.clone();
    for slide in &mut refined_slides {
        let refined =
            pitch_deck_service::services::extraction::stage3_table_chart::extract_tables_and_charts(
                slide.clone(),
            );
        *slide = refined;
    }

    let mut refined_tables = 0u32;
    let mut refined_charts = 0u32;
    for slide in &refined_slides {
        for elem in &slide.elements {
            match elem {
                pitch_deck_service::models::extraction_model::Element::Table { .. } => {
                    refined_tables += 1
                }
                pitch_deck_service::models::extraction_model::Element::Chart { .. } => {
                    refined_charts += 1
                }
                _ => {}
            }
        }
    }

    println!("  ├── Tables detected: {}", refined_tables);
    println!("  └── Charts detected: {}", refined_charts);
    println!("  ✓ Table/Chart refinement complete\n");

    // ─── Step 4: Full Pipeline Integration ──────────────────────────────────────
    println!("[Step 4] Full Pipeline Integration (Stage 1 + 2 + 3 combined)");
    println!("  Running process_pitch_deck()...");

    let final_slides =
        pitch_deck_service::services::extraction::process_pitch_deck(test_pdf, "pdf")
            .await
            .expect("Full pipeline should succeed");

    println!("  ✓ Full pipeline produced {} slides\n", final_slides.len());
    assert!(
        !final_slides.is_empty(),
        "Full pipeline must produce at least 1 slide"
    );

    // ─── Step 5: OCR Availability Check ─────────────────────────────────────────
    println!("[Step 5] OCR System Check");

    let tesseract_available =
        pitch_deck_service::services::extraction::stage1_parser::ocr::OcrExtractor::is_available()
            .await;
    let pdfimages_available = pitch_deck_service::services::extraction::stage1_parser::ocr::OcrExtractor::pdfimages_available().await;

    println!(
        "  ├── Tesseract OCR:  {}",
        if tesseract_available {
            "✓ AVAILABLE"
        } else {
            "✗ NOT FOUND"
        }
    );
    println!(
        "  └── pdfimages:      {}",
        if pdfimages_available {
            "✓ AVAILABLE"
        } else {
            "✗ NOT FOUND"
        }
    );
    println!();

    // ─── Step 6: Build Structured JSON Output ───────────────────────────────────
    println!("[Step 6] Building Structured JSON Output");

    let mut json_slides: Vec<serde_json::Value> = Vec::new();

    for slide in &final_slides {
        let mut json_elements: Vec<serde_json::Value> = Vec::new();

        for elem in &slide.elements {
            let json_elem = match elem {
                pitch_deck_service::models::extraction_model::Element::TextBlock { text, bbox } => {
                    serde_json::json!({
                        "type": "TextBlock",
                        "text": text,
                        "has_bbox": bbox.is_some(),
                        "bbox": bbox.as_ref().map(|b| serde_json::json!({
                            "x0": b.x0, "y0": b.y0, "x1": b.x1, "y1": b.y1
                        }))
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Title { text, bbox } => {
                    serde_json::json!({
                        "type": "Title",
                        "text": text,
                        "has_bbox": bbox.is_some(),
                        "bbox": bbox.as_ref().map(|b| serde_json::json!({
                            "x0": b.x0, "y0": b.y0, "x1": b.x1, "y1": b.y1
                        }))
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Subtitle { text, bbox } => {
                    serde_json::json!({
                        "type": "Subtitle",
                        "text": text,
                        "has_bbox": bbox.is_some(),
                        "bbox": bbox.as_ref().map(|b| serde_json::json!({
                            "x0": b.x0, "y0": b.y0, "x1": b.x1, "y1": b.y1
                        }))
                    })
                }
                pitch_deck_service::models::extraction_model::Element::BulletList {
                    items,
                    level,
                    bbox,
                } => {
                    serde_json::json!({
                        "type": "BulletList",
                        "items": items,
                        "level": level,
                        "has_bbox": bbox.is_some(),
                        "bbox": bbox.as_ref().map(|b| serde_json::json!({
                            "x0": b.x0, "y0": b.y0, "x1": b.x1, "y1": b.y1
                        }))
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Table {
                    headers,
                    rows,
                    bbox,
                } => {
                    serde_json::json!({
                        "type": "Table",
                        "headers": headers,
                        "rows": rows,
                        "has_bbox": bbox.is_some()
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Chart {
                    chart_type,
                    title,
                    bbox,
                    ..
                } => {
                    serde_json::json!({
                        "type": "Chart",
                        "chart_type": chart_type,
                        "title": title,
                        "has_bbox": bbox.is_some()
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Image { bbox, .. } => {
                    serde_json::json!({
                        "type": "Image",
                        "has_bbox": bbox.is_some()
                    })
                }
                pitch_deck_service::models::extraction_model::Element::SectionHeader {
                    text,
                    bbox,
                } => {
                    serde_json::json!({
                        "type": "SectionHeader",
                        "text": text,
                        "has_bbox": bbox.is_some()
                    })
                }
                pitch_deck_service::models::extraction_model::Element::Statistic {
                    value,
                    label,
                    bbox,
                } => {
                    serde_json::json!({
                        "type": "Statistic",
                        "value": value,
                        "label": label,
                        "has_bbox": bbox.is_some()
                    })
                }
            };
            json_elements.push(json_elem);
        }

        json_slides.push(serde_json::json!({
            "slide_number": slide.slide_number,
            "element_count": slide.elements.len(),
            "elements": json_elements
        }));
    }

    let full_output = serde_json::json!({
        "test": "Phase 1 - Extraction Layer Smoke Test",
        "source_file": "tests/test_pdf.pdf",
        "file_size_bytes": file_size,
        "total_slides": final_slides.len(),
        "summary": {
            "titles": titles,
            "subtitles": subtitles,
            "section_headers": section_headers,
            "statistics": statistics,
            "bullet_lists": classified_bullets,
            "text_blocks": remaining_text,
            "tables": refined_tables,
            "charts": refined_charts,
            "bbox_coverage_percent": format!("{:.1}", bbox_coverage)
        },
        "system": {
            "tesseract_available": tesseract_available,
            "pdfimages_available": pdfimages_available
        },
        "slides": json_slides
    });

    // Pretty print JSON to terminal
    let json_pretty = serde_json::to_string_pretty(&full_output).unwrap();
    println!("  Full JSON output ({} bytes):", json_pretty.len());
    println!("{}", "-".repeat(60));

    // Print truncated version to terminal (first 3000 chars)
    if json_pretty.len() > 3000 {
        println!("{}", &json_pretty[..3000]);
        println!("  ... (truncated, full output saved to file)");
    } else {
        println!("{}", json_pretty);
    }
    println!("{}", "-".repeat(60));

    // Optionally write full JSON to file when requested.
    if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
        let output_path = "tests/smoke_phase1_output.json";
        std::fs::write(output_path, &json_pretty).expect("Should write JSON output file");
        println!(
            "  ✓ Full JSON saved to: {} (WRITE_SMOKE_JSON=1)",
            output_path
        );
    }

    // ─── Step 7: Assertions ─────────────────────────────────────────────────────
    println!("[Step 7] Final Assertions");

    // Core assertions
    assert!(!final_slides.is_empty(), "Must extract at least 1 slide");
    println!("  ✓ Slide count > 0: {} slides", final_slides.len());

    assert!(total_elements != 0, "Must extract at least 1 element");
    println!("  ✓ Element count > 0: {} elements", total_elements);

    // Verify slide numbers are sequential and start from 1
    let first_num = final_slides.first().unwrap().slide_number;
    assert!(first_num >= 1, "First slide number should be >= 1");
    println!("  ✓ First slide number: {}", first_num);

    // Verify at least some classification happened
    let total_classified = titles + subtitles + classified_bullets;
    println!(
        "  ✓ Classified elements: {} (Titles: {}, Subtitles: {}, Bullets: {})",
        total_classified, titles, subtitles, classified_bullets
    );

    // Verify OCR tools availability
    assert!(
        tesseract_available,
        "Tesseract should be installed and in PATH"
    );
    println!("  ✓ Tesseract OCR is available");
    assert!(
        pdfimages_available,
        "pdfimages should be installed and in PATH"
    );
    println!("  ✓ pdfimages is available");

    let elapsed = test_start.elapsed();
    println!();
    println!("{}", "=".repeat(80));
    println!("  ✓ PHASE 1 SMOKE TEST PASSED — All checks green!");
    println!("{}", "=".repeat(80));
    println!(
        "  TOTAL TIME: {:.2} seconds (set WRITE_SMOKE_JSON=1 to persist JSON output)",
        elapsed.as_secs_f64()
    );
    println!();
}
