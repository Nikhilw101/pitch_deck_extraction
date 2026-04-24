//! End-to-end smoke test for Phase 1 & Phase 2 with real PDF input and JSON output.
//!
//! Tests:
//! 1. PDF upload and extraction (Phase 1)
//! 2. Embeddings generation and indexing (Phase 2)
//! 3. Semantic search functionality
//!
//! Output: JSON report saved to tests/smoke_test_results.json
//!
//! Run: cargo test --test smoke_test_e2e -- --nocapture

use axum::body::Body;
use axum::extract::Extension;
use axum::http::Request;
mod common;
use pitch_deck_service::config;
use pitch_deck_service::routes;
use pitch_deck_service::services::embedding_service::CohereClient;
use pitch_deck_service::services::embedding_service::EmbeddingService;
use pitch_deck_service::services::llm_service::LlmService;
use pitch_deck_service::services::llm_service::OllamaClient;
use pitch_deck_service::services::pipeline_service::PipelineService;
use pitch_deck_service::services::vector_store_service::VectorStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

const BOUNDARY: &str = "----WebKitFormBoundary7MA4YWxkTrZu0gW";

#[derive(Debug, Serialize, Deserialize)]
struct TestResult {
    test_name: String,
    success: bool,
    status_code: Option<u16>,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_snippet: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlideExtractedText {
    slide_number: u32,
    title: Option<String>,
    subtitle: Option<String>,
    body_text: String,
    content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedTextOutput {
    slides: Vec<SlideExtractedText>,
    full_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SmokeTestReport {
    timestamp: String,
    pdf_file: String,
    pdf_exists: bool,
    tests: Vec<TestResult>,
    extracted_text: Option<ExtractedTextOutput>,
    embedding_details: Option<Value>,
    classification_details: Option<Value>,
    structured_output_details: Option<Value>,
    search_results: Option<Value>,
    final_response: Option<Value>,
    summary: TestSummary,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestSummary {
    total_tests: usize,
    passed: usize,
    failed: usize,
    overall_status: String,
}

/// Build extracted text output from API response data.slides.
/// Note: ProcessingResponse uses #[serde(flatten)], so deck fields are at data.* level
fn build_extracted_text_from_response(data: &Value) -> Option<ExtractedTextOutput> {
    // Try both structures: flattened (data.slides) and nested (data.deck.slides)
    let slides = data
        .get("slides")
        .or_else(|| data.get("deck").and_then(|d| d.get("slides")))
        .and_then(|s| s.as_array())?;
    let mut out_slides = Vec::with_capacity(slides.len());
    let mut full_parts = Vec::new();

    for slide in slides {
        let slide_number = slide
            .get("slide_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32;
        let title = slide
            .get("title")
            .and_then(|t| t.as_str())
            .map(String::from);
        let subtitle = slide
            .get("subtitle")
            .and_then(|s| s.as_str())
            .map(String::from);
        let content = slide
            .get("content")
            .and_then(|c| c.as_str())
            .map(String::from);

        let mut body_parts = Vec::new();
        if let Some(blocks) = slide.get("content_blocks").and_then(|b| b.as_array()) {
            for block in blocks {
                let typ = block.get("type").and_then(|t| t.as_str());
                match typ {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            body_parts.push(t.to_string());
                        }
                    }
                    Some("bulletlist") => {
                        if let Some(items) = block.get("items").and_then(|i| i.as_array()) {
                            for item in items {
                                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                                    body_parts.push(format!("• {}", t));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let body_text = body_parts.join("\n");

        full_parts.push(format!(
            "--- Slide {} ---\n{}\n{}\n{}",
            slide_number,
            title.as_deref().unwrap_or(""),
            subtitle.as_deref().unwrap_or(""),
            body_text
        ));

        out_slides.push(SlideExtractedText {
            slide_number,
            title,
            subtitle,
            body_text,
            content,
        });
    }

    let full_text = full_parts.join("\n\n");
    Some(ExtractedTextOutput {
        slides: out_slides,
        full_text,
    })
}


fn create_test_app() -> Result<axum::Router, Box<dyn std::error::Error>> {
    println!("    → Loading environment variables...");
    dotenvy::dotenv().ok();

    println!("    → Loading configuration...");
    let config = config::load()?;

    println!("    → Creating Cohere embedding service...");
    // Initialize Phase 2 Services
    let embedding_service = Arc::new(CohereClient::new(
        config.cohere_api_key.clone(),
        config.embedding_model.clone(),
    )?);
    println!(
        "      ✓ Embedding service ready (dimension: {})",
        embedding_service.get_dimension()
    );

    println!("    → Creating HNSW vector store...");
    let vector_store = Arc::new(VectorStore::new(
        embedding_service.get_dimension(),
        "tests/tmp_smoke_index.bin".to_string(),
    )?);
    println!("      ✓ Vector store ready");

    println!("    → Creating pipeline service...");
    let pipeline_service = Arc::new(PipelineService::new(
        embedding_service.clone(),
        vector_store.clone(),
    ));
    println!("      ✓ Pipeline service ready");

    println!(
        "    → Creating Ollama LLM service (model: {}, url: {})...",
        config.ollama_model, config.ollama_base_url
    );
    // Initialize Phase 4 LLM Service (Ollama)
    let llm_service = Arc::new(OllamaClient::new(
        Some(config.ollama_model.clone()),
        Some(config.ollama_base_url.clone()),
    )?);
    println!("      ✓ LLM service ready");

    println!("    → Building Axum router with all services...");
    let app = routes::create_router()
        .layer(Extension(
            embedding_service.clone() as Arc<dyn EmbeddingService>
        ))
        .layer(Extension(vector_store.clone()))
        .layer(Extension(pipeline_service.clone()))
        .layer(Extension(llm_service.clone() as Arc<dyn LlmService>));

    println!("      ✓ Router ready");
    Ok(app)
}

#[tokio::test]
async fn smoke_test_e2e_flow() {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let pdf_path = Path::new("tests/test_pdf.pdf");
    let output_path = Path::new("tests/smoke_test_results.json");

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         SMOKE TEST: Phase 1 & Phase 2 E2E Flow              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("  Timestamp: {}", timestamp);
    println!("  PDF File: {}", pdf_path.display());
    println!();

    let mut test_results = Vec::new();

    // Check if PDF exists
    let pdf_exists = pdf_path.exists();
    let pdf_bytes = if pdf_exists {
        match std::fs::read(pdf_path) {
            Ok(bytes) => {
                println!("  ✓ PDF file found ({} bytes)", bytes.len());
                Some(bytes)
            }
            Err(e) => {
                println!("  ✗ Error reading PDF: {}", e);
                None
            }
        }
    } else {
        println!("  ⚠ PDF file not found at {}", pdf_path.display());
        println!("  → Place a test PDF file at tests/test_pdf.pdf to run full test");
        None
    };

    // Initialize app
    println!("  → Initializing services...");
    println!("    - Loading configuration...");
    println!("    - Initializing embedding service (Cohere)...");
    println!("    - Initializing vector store (HNSW)...");
    println!("    - Initializing pipeline service...");
    println!("    - Initializing LLM service (Ollama)...");

    let app = match create_test_app() {
        Ok(app) => {
            println!("  ✓ All services initialized successfully");
            app
        }
        Err(e) => {
            println!("  ✗ Failed to initialize services: {}", e);
            println!("  → Make sure COHERE_API_KEY is set in .env file");
            println!("  → Make sure Ollama is running (ollama serve)");

            test_results.push(TestResult {
                test_name: "Service Initialization".to_string(),
                success: false,
                status_code: None,
                message: format!("Failed: {}", e),
                response_snippet: None,
            });

            let report = SmokeTestReport {
                timestamp,
                pdf_file: pdf_path.display().to_string(),
                pdf_exists,
                tests: test_results,
                extracted_text: None,
                embedding_details: None,
                classification_details: None,
                structured_output_details: None,
                search_results: None,
                final_response: None,
                summary: TestSummary {
                    total_tests: 1,
                    passed: 0,
                    failed: 1,
                    overall_status: "FAILED".to_string(),
                },
            };

            let json = serde_json::to_string_pretty(&report).unwrap();
            if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
                let _ = std::fs::write(output_path, &json);
                println!(
                    "  Report written to {} (WRITE_SMOKE_JSON=1)",
                    output_path.display()
                );
            } else {
                println!(
                    "  Report not written (set WRITE_SMOKE_JSON=1 to persist JSON output at {})",
                    output_path.display()
                );
            }
            return;
        }
    };

    // Variables to store detailed output
    let mut extracted_text: Option<ExtractedTextOutput> = None;
    let mut final_response: Option<Value> = None;
    let mut embedding_details: Option<Value> = None;
    let mut classification_details: Option<Value> = None;
    let mut structured_output_details: Option<Value> = None;
    let mut search_results: Option<Value> = None;

    // Test 1: PDF Upload & Extraction (Phase 1)
    println!("\n[TEST 1] PDF Upload & Extraction (Phase 1)");
    println!("  ──────────────────────────────────────────");
    println!("  → Step 1.1: Preparing multipart request...");

    if let Some(pdf_data) = &pdf_bytes {
        println!(
            "  → Step 1.2: Creating multipart body ({} bytes)...",
            pdf_data.len()
        );
        let body = common::multipart_body(BOUNDARY, "test_pdf.pdf", "application/pdf", pdf_data);
        let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);

        println!("  → Step 1.3: Sending POST request to /api/decks/upload endpoint...");
        let request = Request::builder()
            .method("POST")
            .uri("/api/decks/upload")
            .header("content-type", content_type)
            .body(Body::from(body))
            .unwrap();

        println!("  → Step 1.4: Waiting for server response...");
        let response = app.clone().oneshot(request).await.unwrap();
        println!("  → Step 1.5: Parsing response...");
        let status_code = response.status().as_u16();
        let resp_body = response.into_body();
        let bytes = http_body_util::BodyExt::collect(resp_body)
            .await
            .unwrap()
            .to_bytes();
        let body_str = String::from_utf8_lossy(&bytes);

        println!("  → Step 1.6: Parsing JSON response...");
        let json_response: Option<Value> = serde_json::from_str(&body_str).ok();

        // Store final response
        final_response = json_response.clone();

        let success = status_code == 200
            && json_response
                .as_ref()
                .and_then(|j| j.get("status").and_then(|s| s.as_str()))
                == Some("success");

        if success {
            if let Some(ref json) = json_response {
                // ProcessingResponse uses #[serde(flatten)] so deck fields are at data.* not data.deck.*
                let data = json.get("data");
                let total_slides = data
                    .and_then(|d| d.get("total_slides"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);
                let deck_id = data
                    .and_then(|d| d.get("deck_id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("N/A");

                println!("  ✓ Status: {}", status_code);
                println!("  ✓ Deck ID: {}", deck_id);
                println!("  ✓ Total Slides: {}", total_slides);

                // Build extracted text output
                if let Some(data) = json.get("data") {
                    extracted_text = build_extracted_text_from_response(data);
                }

                // Show first slide preview
                if let Some(slides) = data
                    .and_then(|d| d.get("slides"))
                    .and_then(|s| s.as_array())
                {
                    if let Some(first_slide) = slides.first() {
                        let title = first_slide
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or("(no title)");
                        println!("  ✓ First Slide Title: {}", title);
                    }
                }
            }
        } else {
            println!("  ✗ Status: {}", status_code);
            println!(
                "  ✗ Response: {}",
                body_str.chars().take(200).collect::<String>()
            );
        }

        test_results.push(TestResult {
            test_name: "PDF Upload & Extraction".to_string(),
            success,
            status_code: Some(status_code),
            message: if success {
                "Successfully extracted PDF content".to_string()
            } else {
                format!("Failed with status {}", status_code)
            },
            response_snippet: json_response.as_ref().map(|j| {
                let data = j.get("data");
                serde_json::json!({
                    "status": j.get("status"),
                    "deck_id": data.and_then(|d| d.get("deck_id")),
                    "total_slides": data.and_then(|d| d.get("total_slides")),
                })
            }),
        });

        // Test 2: Embeddings & Indexing (Phase 2)
        println!("\n[TEST 2] Embeddings & Indexing (Phase 2)");
        println!("  ──────────────────────────────────────────");
        println!("  → Step 2.1: Checking indexing status in response...");

        let indexing_success = json_response
            .as_ref()
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("indexing"))
            .and_then(|i| i.get("status"))
            .and_then(|s| s.as_str())
            == Some("indexed");

        if indexing_success {
            if let Some(ref json) = json_response {
                let indexing = json.get("data").and_then(|d| d.get("indexing"));
                let slides_indexed = indexing
                    .and_then(|i| i.get("slides_indexed"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);
                let dimension = indexing
                    .and_then(|i| i.get("embedding_dimension"))
                    .and_then(|d| d.as_u64())
                    .unwrap_or(0);

                // Store embedding details
                embedding_details = indexing.cloned();

                println!("  ✓ Indexing Status: indexed");
                println!("  ✓ Slides Indexed: {}", slides_indexed);
                println!("  ✓ Embedding Dimension: {}", dimension);
            }
        } else {
            println!("  ✗ Indexing Status: failed or not found");
        }

        test_results.push(TestResult {
            test_name: "Embeddings & Indexing".to_string(),
            success: indexing_success,
            status_code: Some(status_code),
            message: if indexing_success {
                "Successfully generated embeddings and indexed vectors".to_string()
            } else {
                "Indexing failed or not completed".to_string()
            },
            response_snippet: json_response
                .as_ref()
                .and_then(|j| j.get("data").and_then(|d| d.get("indexing")).cloned()),
        });

        // Test 3: Section Classification & Grouping (Phase 3)
        println!("\n[TEST 3] Section Classification & Grouping (Phase 3)");
        println!("  ──────────────────────────────────────────");
        println!("  → Step 3.1: Checking classification status in response...");

        let classification_success = json_response
            .as_ref()
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("grouped_deck"))
            .is_some();

        if classification_success {
            if let Some(ref json) = json_response {
                let grouped_deck = json.get("data").and_then(|d| d.get("grouped_deck"));
                let sections_count = grouped_deck
                    .and_then(|gd| gd.get("sections"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let classified_slides = grouped_deck
                    .and_then(|gd| gd.get("classification_metadata"))
                    .and_then(|cm| cm.get("classified_slides"))
                    .and_then(|cs| cs.as_u64())
                    .unwrap_or(0);

                // Store classification details
                classification_details = grouped_deck.cloned();

                println!("  ✓ Classification Status: completed");
                println!("  ✓ Sections Found: {}", sections_count);
                println!("  ✓ Classified Slides: {}", classified_slides);

                // Show section breakdown
                if let Some(sections) = grouped_deck
                    .and_then(|gd| gd.get("sections"))
                    .and_then(|s| s.as_array())
                {
                    for section in sections.iter().take(5) {
                        let section_name = section
                            .get("section_name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("Unknown");
                        let slide_count = section
                            .get("slide_count")
                            .and_then(|c| c.as_u64())
                            .unwrap_or(0);
                        if slide_count > 0 {
                            println!("    - {}: {} slide(s)", section_name, slide_count);
                        }
                    }
                }
            }
        } else {
            println!("  ⚠ Classification Status: not found (may be optional)");
        }

        test_results.push(TestResult {
            test_name: "Section Classification & Grouping".to_string(),
            success: classification_success,
            status_code: Some(status_code),
            message: if classification_success {
                "Successfully classified slides into sections and grouped them".to_string()
            } else {
                "Classification not found or failed".to_string()
            },
            response_snippet: json_response.as_ref().and_then(|j| {
                j.get("data").and_then(|d| d.get("grouped_deck")).map(|gd| {
                    serde_json::json!({
                        "sections_count": gd.get("sections").and_then(|s| s.as_array()).map(|a| a.len()),
                        "classified_slides": gd.get("classification_metadata").and_then(|cm| cm.get("classified_slides")),
                    })
                })
            }),
        });

        // Test 4: LLM Processing - Structured Extraction, Validation, Summaries, Signals
        println!(
            "\n[TEST 4] LLM Processing (Structured Extraction, Validation, Summaries, Signals)"
        );
        println!("  ──────────────────────────────────────────");
        println!("  → Step 4.1: Checking structured output in response...");
        println!("  → Step 4.2: This phase includes:");
        println!("     - Structured JSON extraction via Ollama LLM");
        println!("     - Regex-based validation (currency, percentages, dates)");
        println!("     - Summary generation (1-3 sentences per section)");
        println!("     - Investment signals & red flags extraction");
        println!("  → Step 4.3: Processing...");

        let structured_output_success = json_response
            .as_ref()
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("structured_output"))
            .is_some();

        if structured_output_success {
            println!("  → Step 4.4: Structured output found! Analyzing results...");
            if let Some(ref json) = json_response {
                let structured_output = json.get("data").and_then(|d| d.get("structured_output"));

                // Store structured output details
                structured_output_details = structured_output.cloned();

                println!("  → Step 4.5: Extracting metrics from structured output...");
                let sections_count = structured_output
                    .and_then(|so| so.get("sections"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                let confidence_score = structured_output
                    .and_then(|so| so.get("confidence_score"))
                    .and_then(|c| c.as_f64())
                    .unwrap_or(0.0);

                let overall_signals_count = structured_output
                    .and_then(|so| so.get("overall_signals"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                let overall_red_flags_count = structured_output
                    .and_then(|so| so.get("overall_red_flags"))
                    .and_then(|rf| rf.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                println!("  → Step 4.6: Displaying results...");
                println!("  ✓ Structured Output Status: completed");
                println!("  ✓ Sections Processed: {}", sections_count);
                println!("  ✓ Confidence Score: {:.4}", confidence_score);
                println!("  ✓ Overall Signals: {}", overall_signals_count);
                println!("  ✓ Overall Red Flags: {}", overall_red_flags_count);

                // Show section details
                println!("  → Step 4.7: Analyzing individual sections...");
                if let Some(sections) = structured_output
                    .and_then(|so| so.get("sections"))
                    .and_then(|s| s.as_array())
                {
                    for section in sections.iter().take(3) {
                        let section_name = section
                            .get("section_name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("Unknown");
                        let validation_score = section
                            .get("validation")
                            .and_then(|v| v.get("score"))
                            .and_then(|s| s.as_f64())
                            .unwrap_or(0.0);
                        let signals_count = section
                            .get("signals")
                            .and_then(|s| s.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let red_flags_count = section
                            .get("red_flags")
                            .and_then(|rf| rf.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let has_summary = section.get("summary").and_then(|s| s.as_str()).is_some();

                        println!(
                            "    - {}: Validation={:.2}, Signals={}, RedFlags={}, HasSummary={}",
                            section_name,
                            validation_score,
                            signals_count,
                            red_flags_count,
                            has_summary
                        );
                    }
                }
            }
        } else {
            println!("  ⚠ Structured Output Status: not found (may be optional or LLM processing failed)");
        }

        test_results.push(TestResult {
            test_name: "LLM Processing (Structured Extraction, Validation, Summaries, Signals)".to_string(),
            success: structured_output_success,
            status_code: Some(status_code),
            message: if structured_output_success {
                "Successfully processed through LLM: extracted structured data, validated fields, generated summaries, and extracted signals/red flags".to_string()
            } else {
                "LLM processing not found or failed".to_string()
            },
            response_snippet: json_response.as_ref().and_then(|j| {
                j.get("data").and_then(|d| d.get("structured_output")).map(|so| {
                    serde_json::json!({
                        "sections_count": so.get("sections").and_then(|s| s.as_array()).map(|a| a.len()),
                        "confidence_score": so.get("confidence_score"),
                        "overall_signals_count": so.get("overall_signals").and_then(|s| s.as_array()).map(|a| a.len()),
                        "overall_red_flags_count": so.get("overall_red_flags").and_then(|rf| rf.as_array()).map(|a| a.len()),
                    })
                })
            }),
        });

        // Test 5: Semantic Search
        println!("\n[TEST 5] Semantic Search");
        println!("  ──────────────────────────────────────────");
        println!("  → Step 5.1: Preparing search query...");

        if indexing_success {
            let search_query = serde_json::json!({
                "query": "pitch deck business",
                "limit": 3
            });

            println!("  → Step 5.2: Sending POST request to /api/decks/search endpoint...");
            let request = Request::builder()
                .method("POST")
                .uri("/api/decks/search")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&search_query).unwrap()))
                .unwrap();

            println!("  → Step 5.3: Waiting for search results...");
            let response = app.clone().oneshot(request).await.unwrap();
            let search_status = response.status().as_u16();
            let resp_body = response.into_body();
            let bytes = http_body_util::BodyExt::collect(resp_body)
                .await
                .unwrap()
                .to_bytes();
            let body_str = String::from_utf8_lossy(&bytes);

            let search_json: Option<Value> = serde_json::from_str(&body_str).ok();

            // Store search results
            search_results = search_json.clone();

            let search_success = search_status == 200
                && search_json
                    .as_ref()
                    .and_then(|j| j.get("status").and_then(|s| s.as_str()))
                    == Some("success");

            if search_success {
                if let Some(ref json) = search_json {
                    let results = json
                        .get("data")
                        .and_then(|d| d.get("results"))
                        .and_then(|r| r.as_array());
                    let result_count = results.map(|r| r.len()).unwrap_or(0);

                    println!("  ✓ Status: {}", search_status);
                    println!("  ✓ Results Found: {}", result_count);

                    if let Some(results) = results {
                        for (i, result) in results.iter().take(3).enumerate() {
                            let score = result.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                            let slide_num = result
                                .get("slide_number")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0);
                            let text_snippet = result
                                .get("text_snippet")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .chars()
                                .take(50)
                                .collect::<String>();
                            println!(
                                "    [{}) Slide #{} - Score: {:.4} - {}",
                                i + 1,
                                slide_num,
                                score,
                                text_snippet
                            );
                        }
                    }
                }
            } else {
                println!("  ✗ Status: {}", search_status);
                println!(
                    "  ✗ Response: {}",
                    body_str.chars().take(200).collect::<String>()
                );
            }

            test_results.push(TestResult {
                test_name: "Semantic Search".to_string(),
                success: search_success,
                status_code: Some(search_status),
                message: if search_success {
                    "Successfully performed semantic search".to_string()
                } else {
                    format!("Search failed with status {}", search_status)
                },
                response_snippet: search_json.as_ref().map(|j| {
                    serde_json::json!({
                        "status": j.get("status"),
                        "result_count": j.get("data").and_then(|d| d.get("results")).and_then(|r| r.as_array()).map(|a| a.len()),
                    })
                }),
            });
        } else {
            println!("  ⚠ Skipped (indexing must succeed first)");
            test_results.push(TestResult {
                test_name: "Semantic Search".to_string(),
                success: false,
                status_code: None,
                message: "Skipped - indexing failed".to_string(),
                response_snippet: None,
            });
        }
    } else {
        println!("  ⚠ Skipped (no PDF file)");
        test_results.push(TestResult {
            test_name: "PDF Upload & Extraction".to_string(),
            success: false,
            status_code: None,
            message: "Skipped - PDF file not found".to_string(),
            response_snippet: None,
        });
        test_results.push(TestResult {
            test_name: "Embeddings & Indexing".to_string(),
            success: false,
            status_code: None,
            message: "Skipped - PDF file not found".to_string(),
            response_snippet: None,
        });
        test_results.push(TestResult {
            test_name: "Section Classification & Grouping".to_string(),
            success: false,
            status_code: None,
            message: "Skipped - PDF file not found".to_string(),
            response_snippet: None,
        });
        test_results.push(TestResult {
            test_name: "LLM Processing (Structured Extraction, Validation, Summaries, Signals)"
                .to_string(),
            success: false,
            status_code: None,
            message: "Skipped - PDF file not found".to_string(),
            response_snippet: None,
        });
        test_results.push(TestResult {
            test_name: "Semantic Search".to_string(),
            success: false,
            status_code: None,
            message: "Skipped - PDF file not found".to_string(),
            response_snippet: None,
        });
    }

    // Generate summary
    let passed = test_results.iter().filter(|t| t.success).count();
    let failed = test_results.len() - passed;
    let overall_status = if failed == 0 && pdf_exists {
        "PASS"
    } else if !pdf_exists {
        "SKIP"
    } else {
        "FAIL"
    };

    let summary = TestSummary {
        total_tests: test_results.len(),
        passed,
        failed,
        overall_status: overall_status.to_string(),
    };

    // Print summary once (no duplicate boxes)
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                      TEST SUMMARY                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("  Overall Status: {}", summary.overall_status);
    println!("  Tests Passed: {}/{}", summary.passed, summary.total_tests);
    println!("  Tests Failed: {}", summary.failed);
    println!("  Report Saved: {}", output_path.display());

    let report = SmokeTestReport {
        timestamp,
        pdf_file: pdf_path.display().to_string(),
        pdf_exists,
        tests: test_results,
        extracted_text,
        embedding_details,
        classification_details,
        structured_output_details,
        search_results,
        final_response,
        summary,
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
        let _ = std::fs::write(output_path, &json);
        println!(
            "  Report saved to {} (WRITE_SMOKE_JSON=1)",
            output_path.display()
        );
    } else {
        println!(
            "  Report not written (set WRITE_SMOKE_JSON=1 to persist at {})",
            output_path.display()
        );
    }
    println!();

    // Assert at least one test passed if PDF exists
    if pdf_exists {
        assert!(
            passed > 0,
            "At least one test must pass when PDF is available"
        );
    }
}
