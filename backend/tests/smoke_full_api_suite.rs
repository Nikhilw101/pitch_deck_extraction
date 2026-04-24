//! Comprehensive end-to-end API smoke test.
//!
//! Covers:
//! - Config loading and service initialization
//! - Deck upload + full processing pipeline via POST /api/decks/upload
//! - Semantic search via POST /api/decks/search
//! - Structured per-phase JSON report (optional)
//!
//! Usage:
//!   # Basic run (terminal-only output)
//!   cargo test --test smoke_full_api_suite -- --nocapture
//!
//!   # Run and persist JSON report to tests/smoke_full_api_report.json
//!   # (PowerShell)
//!   $env:WRITE_SMOKE_JSON = "1"
//!   cargo test --test smoke_full_api_suite -- --nocapture
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
use tower::ServiceExt;

use pitch_deck_service::config;
use pitch_deck_service::routes;
use pitch_deck_service::services::embedding_service::{CohereClient, EmbeddingService};
use pitch_deck_service::services::job_service::JobService;
use pitch_deck_service::services::llm_service::{LlmService, OllamaClient};
use pitch_deck_service::services::pipeline_service::PipelineService;
use pitch_deck_service::services::vector_store_service::VectorStore;

const BOUNDARY: &str = "----FullApiSmokeBoundary";

#[derive(Debug, Serialize)]
struct EndpointDescriptor {
    name: String,
    method: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct PhaseReport {
    name: String,
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<EndpointDescriptor>,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
    duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

#[derive(Debug, Serialize)]
struct ApiSmokeSummary {
    total_phases: usize,
    passed: usize,
    failed: usize,
    overall_status: String,
}

#[derive(Debug, Serialize)]
struct ApiSmokeReport {
    run_timestamp: String,
    total_duration_ms: u128,
    pdf_file: String,
    endpoints: Vec<EndpointDescriptor>,
    phases: Vec<PhaseReport>,
    summary: ApiSmokeSummary,
}

#[tokio::test]
async fn smoke_full_api_suite() {
    dotenvy::dotenv().ok();
    pitch_deck_service::utils::logger::init();

    let run_start = Instant::now();
    let run_timestamp = chrono::Utc::now().to_rfc3339();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pdf_path = manifest_dir.join("tests").join("test_pdf.pdf");

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║            FULL API SMOKE TEST (backend only)                ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("  Started at: {}", run_timestamp);
    println!("  PDF file:   {}", pdf_path.display());
    println!();

    let endpoints = vec![
        EndpointDescriptor {
            name: "Upload Deck (full pipeline)".to_string(),
            method: "POST".to_string(),
            path: "/api/decks/upload".to_string(),
        },
        EndpointDescriptor {
            name: "Semantic Search".to_string(),
            method: "POST".to_string(),
            path: "/api/decks/search".to_string(),
        },
    ];

    let mut phases: Vec<PhaseReport> = Vec::new();

    //
    // Phase 1: Config load
    //
    println!("[PHASE 1] Loading configuration...");
    let phase_start = Instant::now();
    let (config_opt, phase1) = match config::load() {
        Ok(cfg) => {
            let elapsed = phase_start.elapsed();
            println!(
                "    ✓ Config loaded (port={}, embedding_model={}, ollama_model={}) in {:.2?}",
                cfg.server_port, cfg.embedding_model, cfg.ollama_model, elapsed
            );
            (
                Some(cfg),
                PhaseReport {
                    name: "Load configuration".to_string(),
                    phase: "config".to_string(),
                    endpoint: None,
                    success: true,
                    status_code: None,
                    duration_ms: elapsed.as_millis(),
                    error_message: None,
                    request_id: None,
                    details: None,
                },
            )
        }
        Err(e) => {
            let elapsed = phase_start.elapsed();
            println!("    ✗ Config load failed in {:.2?}", elapsed);
            println!("      Error: {}", e);
            (
                None,
                PhaseReport {
                    name: "Load configuration".to_string(),
                    phase: "config".to_string(),
                    endpoint: None,
                    success: false,
                    status_code: None,
                    duration_ms: elapsed.as_millis(),
                    error_message: Some(format!(
                        "Config load failed: {}. Make sure COHERE_API_KEY and other env vars are set.",
                        e
                    )),
                    request_id: None,
                    details: None,
                },
            )
        }
    };
    phases.push(phase1);

    // If config failed, skip the rest but still write report and fail at the end.
    let cfg = if let Some(c) = config_opt {
        c
    } else {
        finalize_and_assert(run_start, &pdf_path, endpoints, phases);
        return;
    };

    //
    // Phase 2: Service initialization (embedding, vector store, pipeline, LLM, router)
    //
    println!("\n[PHASE 2] Initializing services (Cohere, HNSW, pipeline, Ollama)...");
    let phase_start = Instant::now();

    let (app_opt, phase2) = (|| -> Result<axum::Router, Box<dyn std::error::Error>> {
        let embedding_service = Arc::new(CohereClient::new(
            cfg.cohere_api_key.clone(),
            cfg.embedding_model.clone(),
        )?) as Arc<dyn EmbeddingService>;

        let vector_store = Arc::new(VectorStore::new(
            embedding_service.get_dimension(),
            "tests/tmp_full_api_smoke_index".to_string(),
        )?);

        let pipeline_service =
            Arc::new(PipelineService::new(embedding_service.clone(), vector_store.clone()));

        let llm_service = Arc::new(
            OllamaClient::new(
                Some(cfg.ollama_model.clone()),
                Some(cfg.ollama_base_url.clone()),
            )?,
        ) as Arc<dyn LlmService>;
        let job_service = Arc::new(JobService::new());

        let app = routes::create_router()
            .layer(Extension(embedding_service as Arc<dyn EmbeddingService>))
            .layer(Extension(vector_store))
            .layer(Extension(pipeline_service))
            .layer(Extension(llm_service as Arc<dyn LlmService>))
            .layer(Extension(job_service));

        Ok(app)
    })()
    .map_or_else(
        |e| {
            let elapsed = phase_start.elapsed();
            println!("    ✗ Service initialization failed in {:.2?}", elapsed);
            println!("      Error: {}", e);
            (
                None,
                PhaseReport {
                    name: "Initialize services".to_string(),
                    phase: "init_services".to_string(),
                    endpoint: None,
                    success: false,
                    status_code: None,
                    duration_ms: elapsed.as_millis(),
                    error_message: Some(format!(
                        "Service initialization failed: {}. Check COHERE_API_KEY and that Ollama is running.",
                        e
                    )),
                    request_id: None,
                    details: None,
                },
            )
        },
        |app| {
            let elapsed = phase_start.elapsed();
            println!("    ✓ All services initialized in {:.2?}", elapsed);
            (
                Some(app),
                PhaseReport {
                    name: "Initialize services".to_string(),
                    phase: "init_services".to_string(),
                    endpoint: None,
                    success: true,
                    status_code: None,
                    duration_ms: elapsed.as_millis(),
                    error_message: None,
                    request_id: None,
                    details: None,
                },
            )
        },
    );
    phases.push(phase2);

    let app = if let Some(a) = app_opt {
        a
    } else {
        finalize_and_assert(run_start, &pdf_path, endpoints, phases);
        return;
    };

    //
    // Phase 3: Deck upload + full pipeline via /api/decks/upload
    //
    println!("\n[PHASE 3] Upload deck and run full pipeline (POST /api/decks/upload)...");
    let phase_start = Instant::now();

    if !pdf_path.exists() {
        let elapsed = phase_start.elapsed();
        println!("    ✗ PDF not found at {}", pdf_path.display());
        phases.push(PhaseReport {
            name: "Upload deck".to_string(),
            phase: "upload".to_string(),
            endpoint: Some(EndpointDescriptor {
                name: "Upload Deck (full pipeline)".to_string(),
                method: "POST".to_string(),
                path: "/api/decks/upload".to_string(),
            }),
            success: false,
            status_code: None,
            duration_ms: elapsed.as_millis(),
            error_message: Some(format!(
                "Missing tests/test_pdf.pdf at {}",
                pdf_path.display()
            )),
            request_id: None,
            details: None,
        });

        finalize_and_assert(run_start, &pdf_path, endpoints, phases);
        return;
    }

    let pdf_bytes =
        std::fs::read(&pdf_path).expect("Failed to read tests/test_pdf.pdf for API smoke test");
    println!("    → PDF size: {} bytes", pdf_bytes.len());

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

    println!("    → Sending POST /api/decks/upload ...");
    let upload_resp = app
        .clone()
        .oneshot(upload_req)
        .await
        .expect("upload request failed");
    let status = upload_resp.status();
    let status_code = status.as_u16();
    let upload_bytes = BodyExt::collect(upload_resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let upload_str = String::from_utf8_lossy(&upload_bytes);
    let upload_json: Option<Value> = serde_json::from_str(&upload_str).ok();

    let (upload_success, upload_request_id, upload_details) = if status == StatusCode::OK
        && upload_json
            .as_ref()
            .and_then(|j| j.get("status").and_then(Value::as_str))
            == Some("success")
    {
        let data = upload_json.as_ref().and_then(|j| j.get("data"));
        let total_slides = data
            .and_then(|d| d.get("total_slides"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let indexing_status = data
            .and_then(|d| d.get("indexing"))
            .and_then(|i| i.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let structured_present = data.and_then(|d| d.get("structured_output")).is_some();

        let req_id = upload_json
            .as_ref()
            .and_then(|j| j.get("request_id"))
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        println!("    ✓ Upload succeeded with status {}", status_code);
        println!("      - total_slides: {}", total_slides);
        println!("      - indexing.status: {}", indexing_status);
        println!("      - structured_output present: {}", structured_present);
        if let Some(ref rid) = req_id {
            println!("      - request_id: {}", rid);
        }

        (
            true,
            req_id,
            Some(serde_json::json!({
                "status": "success",
                "status_code": status_code,
                "total_slides": total_slides,
                "indexing_status": indexing_status,
                "structured_output_present": structured_present,
            })),
        )
    } else {
        println!("    ✗ Upload failed with status {}", status_code);
        println!(
            "      Response (first 200 chars): {}",
            upload_str.chars().take(200).collect::<String>()
        );

        let req_id = upload_json
            .as_ref()
            .and_then(|j| j.get("request_id"))
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        (
            false,
            req_id,
            Some(serde_json::json!({
                "status": "error",
                "status_code": status_code,
                "response_excerpt": upload_str.chars().take(200).collect::<String>(),
            })),
        )
    };

    let elapsed = phase_start.elapsed();
    phases.push(PhaseReport {
        name: "Upload deck".to_string(),
        phase: "upload".to_string(),
        endpoint: Some(EndpointDescriptor {
            name: "Upload Deck (full pipeline)".to_string(),
            method: "POST".to_string(),
            path: "/api/decks/upload".to_string(),
        }),
        success: upload_success,
        status_code: Some(status_code),
        duration_ms: elapsed.as_millis(),
        error_message: if upload_success {
            None
        } else {
            Some("Upload via /api/decks/upload failed".to_string())
        },
        request_id: upload_request_id,
        details: upload_details,
    });

    if !upload_success {
        finalize_and_assert(run_start, &pdf_path, endpoints, phases);
        return;
    }

    //
    // Phase 4: Semantic search via /api/decks/search
    //
    println!("\n[PHASE 4] Semantic search (POST /api/decks/search)...");
    let phase_start = Instant::now();

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

    println!("    → Sending POST /api/decks/search ...");
    let search_resp = app
        .clone()
        .oneshot(search_req)
        .await
        .expect("search request failed");
    let search_status = search_resp.status();
    let search_status_code = search_status.as_u16();
    let search_bytes = BodyExt::collect(search_resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let search_str = String::from_utf8_lossy(&search_bytes);
    let search_json: Option<Value> = serde_json::from_str(&search_str).ok();

    let (search_success, search_details) = if search_status == StatusCode::OK
        && search_json
            .as_ref()
            .and_then(|j| j.get("status").and_then(Value::as_str))
            == Some("success")
    {
        let results = search_json
            .as_ref()
            .and_then(|j| j.get("data"))
            .and_then(|d| d.get("results"))
            .and_then(Value::as_array);
        let result_count = results.map(|r| r.len()).unwrap_or(0);
        println!("    ✓ Search succeeded with status {}", search_status_code);
        println!("      - results: {}", result_count);

        if let Some(results) = results {
            for (i, r) in results.iter().take(3).enumerate() {
                let score = r.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                let slide_number = r.get("slide_number").and_then(Value::as_u64).unwrap_or(0);
                let text_snippet = r
                    .get("text_snippet")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect::<String>();
                println!(
                    "        [{}] slide #{}, score={:.4}, text=\"{}\"",
                    i + 1,
                    slide_number,
                    score,
                    text_snippet
                );
            }
        }

        (
            true,
            Some(serde_json::json!({
                "status": "success",
                "status_code": search_status_code,
                "result_count": result_count,
            })),
        )
    } else {
        println!("    ✗ Search failed with status {}", search_status_code);
        println!(
            "      Response (first 200 chars): {}",
            search_str.chars().take(200).collect::<String>()
        );
        (
            false,
            Some(serde_json::json!({
                "status": "error",
                "status_code": search_status_code,
                "response_excerpt": search_str.chars().take(200).collect::<String>(),
            })),
        )
    };

    let elapsed = phase_start.elapsed();
    phases.push(PhaseReport {
        name: "Semantic search".to_string(),
        phase: "search".to_string(),
        endpoint: Some(EndpointDescriptor {
            name: "Semantic Search".to_string(),
            method: "POST".to_string(),
            path: "/api/decks/search".to_string(),
        }),
        success: search_success,
        status_code: Some(search_status_code),
        duration_ms: elapsed.as_millis(),
        error_message: if search_success {
            None
        } else {
            Some("Search via /api/decks/search failed".to_string())
        },
        request_id: None,
        details: search_details,
    });

    finalize_and_assert(run_start, &pdf_path, endpoints, phases);
}

fn finalize_and_assert(
    run_start: Instant,
    pdf_path: &Path,
    endpoints: Vec<EndpointDescriptor>,
    phases: Vec<PhaseReport>,
) {
    let total_duration = run_start.elapsed();
    let passed = phases.iter().filter(|p| p.success).count();
    let failed = phases.len().saturating_sub(passed);
    let overall_status = if failed == 0 { "PASS" } else { "FAIL" }.to_string();

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        API SMOKE SUMMARY                     ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("  Overall: {}", overall_status);
    println!("  Phases:  {} passed / {} total", passed, phases.len());
    println!(
        "  Total time: {:.2} seconds ({:.2} minutes)",
        total_duration.as_secs_f64(),
        total_duration.as_secs_f64() / 60.0
    );
    println!();

    for p in &phases {
        println!(
            "  - [{}] {} (phase={}, duration={} ms{})",
            if p.success { "OK" } else { "!!" },
            p.name,
            p.phase,
            p.duration_ms,
            p.status_code
                .map(|c| format!(", status={}", c))
                .unwrap_or_default()
        );
        if let Some(ref msg) = p.error_message {
            println!("      Error: {}", msg);
        }
    }

    let report = ApiSmokeReport {
        run_timestamp: chrono::Utc::now().to_rfc3339(),
        total_duration_ms: total_duration.as_millis(),
        pdf_file: pdf_path.display().to_string(),
        endpoints,
        phases,
        summary: ApiSmokeSummary {
            total_phases: passed + failed,
            passed,
            failed,
            overall_status,
        },
    };

    if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
        let output_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("smoke_full_api_report.json");
        match std::fs::write(
            &output_path,
            serde_json::to_string_pretty(&report).expect("Failed to serialize report JSON"),
        ) {
            Ok(_) => {
                println!(
                    "\n  JSON report written to {} (WRITE_SMOKE_JSON=1)",
                    output_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "\n  WARNING: Failed to write JSON report to {}: {}",
                    output_path.display(),
                    e
                );
            }
        }
    } else {
        println!(
            "\n  JSON report not written (set WRITE_SMOKE_JSON=1 to persist to tests/smoke_full_api_report.json)"
        );
    }

    assert!(
        failed == 0,
        "Some API smoke test phases failed. See terminal output (and JSON report if enabled) for details."
    );
}
