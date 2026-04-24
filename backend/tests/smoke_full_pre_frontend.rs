//! Single end-to-end smoke test to run **before** frontend integration.
//!
//! Flow covered (for a real PDF deck in `tests/test_pdf.pdf`):
//!   PDF
//!    ↓
//!   Text Extraction (slide-wise)
//!    ↓
//!   Embeddings Generation (Cohere)
//!    ↓
//!   HNSW Vector Index (FAISS-like)
//!    ↓
//!   Section Classification
//!    ↓
//!   Structured JSON via Local LLM (Ollama)
//!    ↓
//!   Regex + Numeric Validation
//!    ↓
//!   Summaries + Signals + Red Flags
//!    ↓
//!   Semantic Search over indexed deck
//!
//! Run with:
//!   cargo test --test smoke_full_pre_frontend -- --nocapture
//!

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Extension;
use http_body_util::BodyExt;
mod common;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

use pitch_deck_service::config;
use pitch_deck_service::routes;
use pitch_deck_service::services::embedding_service::{CohereClient, EmbeddingService};
use pitch_deck_service::services::llm_service::{LlmService, OllamaClient};
use pitch_deck_service::services::pipeline_service::PipelineService;
use pitch_deck_service::services::vector_store_service::VectorStore;

const BOUNDARY: &str = "----FullSmokeBoundary";

#[derive(Serialize)]
struct SmokePhaseSummary {
    name: &'static str,
    status: &'static str,
    duration_secs: f64,
}

#[derive(Serialize)]
struct SmokeTestSummary {
    overall_status: &'static str,
    total_duration_secs: f64,
    phases: Vec<SmokePhaseSummary>,
}

async fn run_phase_with_timer<F, T>(
    phase_name: &str,
    phase_index: usize,
    phase_total: usize,
    expected_seconds: u64,
    phase_future: F,
) -> (T, f64)
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    println!(
        ">>> [PHASE {}/{}] {} - started",
        phase_index, phase_total, phase_name
    );

    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    let name = phase_name.to_string();

    let ticker = tokio::spawn(async move {
        let mut remaining = expected_seconds;
        loop {
            if done_flag.load(Ordering::Relaxed) {
                break;
            }
            println!("    [{}] {}s remaining...", name, remaining);
            remaining = remaining.saturating_sub(1);
            sleep(Duration::from_secs(1)).await;
        }
    });

    let start = Instant::now();
    let result = phase_future.await;
    let duration = start.elapsed().as_secs_f64();

    done.store(true, Ordering::Relaxed);
    let _ = ticker.await;

    println!(
        ">>> [PHASE {}/{}] {} - finished in {:.2}s",
        phase_index, phase_total, phase_name, duration
    );

    (result, duration)
}


#[tokio::test]
async fn smoke_full_pipeline_before_frontend() {
    // Load env and config (requires at least COHERE_API_KEY; Ollama optional but recommended)
    dotenvy::dotenv().ok();
    pitch_deck_service::utils::logger::init();
    let config = match config::load() {
        Ok(c) => c,
        Err(e) => {
            panic!(
                "Config load failed: {}. Make sure COHERE_API_KEY and other env vars are set.",
                e
            );
        }
    };

    // Prepare real test PDF
    let pdf_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_pdf.pdf");
    if !pdf_path.exists() {
        panic!(
            "Missing tests/test_pdf.pdf. Add a realistic pitch deck there before running this smoke test."
        );
    }
    let pdf_bytes =
        std::fs::read(&pdf_path).expect("Failed to read tests/test_pdf.pdf for smoke test");

    // Build services (Phase 2, vector index, pipeline, LLM) and router
    let embedding_service = Arc::new(
        CohereClient::new(
            config.cohere_api_key.clone(),
            config.embedding_model.clone(),
        )
        .expect("Failed to init Cohere embedding service"),
    ) as Arc<dyn EmbeddingService>;

    let vector_store = Arc::new(
        VectorStore::new(
            embedding_service.get_dimension(),
            "tests/tmp_full_smoke_index".to_string(),
        )
        .expect("Failed to init VectorStore"),
    );

    let pipeline_service = Arc::new(PipelineService::new(
        embedding_service.clone(),
        vector_store.clone(),
    ));

    let llm_service = Arc::new(
        OllamaClient::new(
            Some(config.ollama_model.clone()),
            Some(config.ollama_base_url.clone()),
        )
        .expect("Failed to init Ollama LLM client"),
    ) as Arc<dyn LlmService>;

    let app = routes::create_router()
        .layer(Extension(embedding_service as Arc<dyn EmbeddingService>))
        .layer(Extension(vector_store))
        .layer(Extension(pipeline_service))
        .layer(Extension(llm_service as Arc<dyn LlmService>));

    let test_start = Instant::now();

    //
    // 1) Upload PDF → extraction + embeddings + index + classification + LLM flow
    //
    let upload_body = common::multipart_body(BOUNDARY, "test_pdf.pdf", "application/pdf", &pdf_bytes);
    let upload_req = Request::builder()
        .method("POST")
        .uri("/api/decks/upload")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(Body::from(upload_body))
        .unwrap();

    let app_for_upload = app.clone();

    let (upload_json, phase1_secs) = run_phase_with_timer(
        "Upload + extraction + embeddings + LLM pipeline",
        1,
        2,
        600,
        async move {
            let upload_resp = app_for_upload
                .oneshot(upload_req)
                .await
                .expect("upload request failed");

            assert_eq!(
                upload_resp.status(),
                StatusCode::OK,
                "Upload must return 200 for valid test deck"
            );

            let upload_bytes = BodyExt::collect(upload_resp.into_body())
                .await
                .unwrap()
                .to_bytes();
            let upload_json: Value =
                serde_json::from_slice(&upload_bytes).expect("Upload response must be valid JSON");

            // Always write full upload response to disk for inspection.
            let mut file = File::create("tests/smoke_test_upload_output.json")
                .expect("Failed to create JSON output file");
            file.write_all(
                serde_json::to_string_pretty(&upload_json)
                    .expect("Failed to serialize upload JSON")
                    .as_bytes(),
            )
            .expect("Failed to write JSON output");

            upload_json
        },
    )
    .await;

    // Basic top-level contract
    assert_eq!(
        upload_json.get("status").and_then(Value::as_str),
        Some("success"),
        "Upload must return status=success"
    );
    let data = upload_json
        .get("data")
        .expect("Upload response must include data object");

    // 1.a) Extraction sanity (indexing)
    let indexing = data
        .get("indexing")
        .expect("data.indexing must be present in upload response");
    assert_eq!(
        indexing.get("status").and_then(Value::as_str),
        Some("indexed"),
        "Indexing status must be 'indexed'"
    );
    // Keep terminal output minimal; we only print final JSON locations + total time.

    // 1.c) Section classification (now optional in grouped_deck field)
    let grouped_deck = data.get("grouped_deck");
    if let Some(gd) = grouped_deck {
        if !gd.is_null() {
            let sections = gd
                .get("sections")
                .and_then(Value::as_array)
                .expect("grouped_deck.sections must be an array");
            assert!(
                !sections.is_empty(),
                "At least one section must be created during classification"
            );
        }
    }

    // 1.d) Structured JSON + validation + summaries + signals
    let structured_output = data
        .get("structured_output")
        .expect("data.structured_output must be present after LLM/validation pipeline");

    let score_breakdown = structured_output
        .get("score_breakdown")
        .expect("structured_output.score_breakdown must be present");

    let final_score = score_breakdown
        .get("final_score")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    assert!(
        (0.0..=1.0).contains(&final_score),
        "Final score in score_breakdown must be between 0.0 and 1.0"
    );

    assert!(
        score_breakdown.get("validation_score").is_some()
            && score_breakdown.get("llm_confidence").is_some()
            && score_breakdown.get("completeness_score").is_some(),
        "Score breakdown must include validation, LLM, and completeness scores"
    );

    let so_sections = structured_output
        .get("sections")
        .and_then(Value::as_array)
        .expect("structured_output.sections must be an array");
    assert!(
        !so_sections.is_empty(),
        "structured_output.sections must not be empty"
    );

    // Check that at least one section has a summary and either signals or red flags
    let mut found_rich_section = false;
    for s in so_sections {
        let has_summary = s
            .get("summary")
            .and_then(Value::as_str)
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);
        let signals_count = s
            .get("signals")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        let red_flags_count = s
            .get("red_flags")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        if has_summary && (signals_count > 0 || red_flags_count > 0) {
            found_rich_section = true;
            break;
        }
    }
    assert!(
        found_rich_section,
        "At least one section must have a summary and at least one signal or red flag"
    );

    // Verify section summaries: at least one section has a non-empty summary
    let section_summary_count = so_sections
        .iter()
        .filter(|s| {
            s.get("summary")
                .and_then(Value::as_str)
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false)
        })
        .count();
    assert!(
        section_summary_count >= 1,
        "At least one section must have a non-empty summary"
    );

    // Verify overall deck summary: key must exist; value should be non-empty with updated prompts
    assert!(
        structured_output.get("overall_summary").is_some(),
        "structured_output.overall_summary key must be present"
    );
    let overall_summary = structured_output
        .get("overall_summary")
        .and_then(Value::as_str)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    assert!(
        overall_summary.is_some(),
        "overall_summary must be non-empty (executive summary for the whole deck)"
    );
    // Keep terminal output minimal; we only print final JSON locations + total time.

    //
    // 2) Semantic search over indexed deck
    //
    let app_for_search = app.clone();

    let (_, phase2_secs) =
        run_phase_with_timer("Semantic search over indexed deck", 2, 2, 120, async move {
            let search_query = serde_json::json!({
                "query": "revenue growth",
                "limit": 3
            });
            let search_req = Request::builder()
                .method("POST")
                .uri("/api/decks/search")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&search_query).unwrap()))
                .unwrap();

            let search_resp = app_for_search
                .oneshot(search_req)
                .await
                .expect("search request failed");

            assert_eq!(
                search_resp.status(),
                StatusCode::OK,
                "Search must return 200 when index is ready"
            );

            let search_bytes = BodyExt::collect(search_resp.into_body())
                .await
                .unwrap()
                .to_bytes();
            let search_json: Value =
                serde_json::from_slice(&search_bytes).expect("Search response must be valid JSON");

            // Always write full search response to disk for inspection.
            let mut file = File::create("tests/smoke_test_search_output.json")
                .expect("Failed to create JSON search output file");
            file.write_all(
                serde_json::to_string_pretty(&search_json)
                    .expect("Failed to serialize search JSON")
                    .as_bytes(),
            )
            .expect("Failed to write JSON output");

            assert_eq!(
                search_json.get("status").and_then(Value::as_str),
                Some("success"),
                "Search response must have status=success"
            );

            let results = search_json
                .get("data")
                .and_then(|d| d.get("results"))
                .and_then(Value::as_array)
                .expect("Search response must include data.results array");

            // We accept 0 results (depending on deck content) but if there are results, they must have score.
            if let Some(first) = results.first() {
                assert!(
                    first.get("score").and_then(Value::as_f64).is_some(),
                    "Each search result must include a numeric score"
                );
            }
        })
        .await;

    let total_elapsed = test_start.elapsed().as_secs_f64();

    let summary = SmokeTestSummary {
        overall_status: "passed",
        total_duration_secs: total_elapsed,
        phases: vec![
            SmokePhaseSummary {
                name: "upload_and_processing",
                status: "ok",
                duration_secs: phase1_secs,
            },
            SmokePhaseSummary {
                name: "semantic_search",
                status: "ok",
                duration_secs: phase2_secs,
            },
        ],
    };

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let summary_path = manifest
        .join("tests")
        .join("smoke_full_pre_frontend_summary.json");

    let mut file = File::create(&summary_path).expect("Failed to create smoke summary JSON");
    file.write_all(
        serde_json::to_string_pretty(&summary)
            .expect("Failed to serialize smoke summary JSON")
            .as_bytes(),
    )
    .expect("Failed to write smoke summary JSON");

    println!(
        "Smoke test PASSED. Summary JSON written to: {}",
        summary_path.display()
    );
}
