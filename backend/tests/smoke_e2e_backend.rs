use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Extension;
mod common;
use pitch_deck_service::config;
use pitch_deck_service::services::embedding_service::{CohereClient, EmbeddingService};
use pitch_deck_service::services::job_service::JobService;
use pitch_deck_service::services::llm_service::{LlmService, OllamaClient};
use pitch_deck_service::services::pipeline_service::PipelineService;
use pitch_deck_service::services::vector_store_service::VectorStore;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const BOUNDARY: &str = "----BackendSmokeTestBoundary";

#[tokio::test]
async fn test_e2e_backend_flow() {
    dotenvy::dotenv().ok();
    let config = config::load().expect("Config load failed (need COHERE_API_KEY etc.)");

    let embedding_service = Arc::new(
        CohereClient::new(
            config.cohere_api_key.clone(),
            config.embedding_model.clone(),
        )
        .expect("Failed to init Cohere"),
    ) as Arc<dyn EmbeddingService>;
    let vector_store = Arc::new(
        VectorStore::new(
            embedding_service.get_dimension(),
            "tests/tmp_smoke_index".to_string(),
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
        .expect("Failed to init Ollama"),
    ) as Arc<dyn LlmService>;
    let job_service = Arc::new(JobService::new());

    let app = pitch_deck_service::routes::create_router()
        .layer(Extension(embedding_service.clone()))
        .layer(Extension(vector_store))
        .layer(Extension(pipeline_service))
        .layer(Extension(llm_service))
        .layer(Extension(job_service));

    // 1. Test PDF Upload & Indexing
    let pdf_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_pdf.pdf");
    if !pdf_path.exists() {
        eprintln!("Skipping PDF upload test: {:?} not found", pdf_path);
    } else {
        let pdf_data = std::fs::read(&pdf_path).expect("read test PDF");
        let body = common::multipart_body(BOUNDARY, "test_pdf.pdf", "application/pdf", &pdf_data);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/decks/upload")
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={}", BOUNDARY),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let body_bytes = axum::body::to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();

        if status != StatusCode::OK {
            println!("Error response: {}", String::from_utf8_lossy(&body_bytes));
        }

        assert_eq!(status, StatusCode::OK);
        let json: Value = serde_json::from_slice(&body_bytes).unwrap();
        println!(
            "PDF Upload Response: {}",
            serde_json::to_string_pretty(&json).unwrap()
        );
        assert_eq!(json["status"], "success");
        let job_id = json["data"]["job_id"]
            .as_str()
            .expect("upload response must include job_id");

        // Upload is async now: poll job status until completion.
        let mut final_payload: Option<Value> = None;
        let mut last_status_payload: Option<Value> = None;
        for _ in 0..120 {
            let status_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/api/jobs/status/{}", job_id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(status_response.status(), StatusCode::OK);
            let status_body = axum::body::to_bytes(status_response.into_body(), 10 * 1024 * 1024)
                .await
                .unwrap();
            let status_json: Value = serde_json::from_slice(&status_body).unwrap();
            last_status_payload = Some(status_json.clone());

            if status_json["data"]["Completed"].is_object() {
                final_payload = status_json["data"]["Completed"].as_object().map(|v| v.clone().into());
                break;
            }

            if status_json["data"]["Failed"].is_object() {
                panic!(
                    "Job failed during smoke test: {}",
                    serde_json::to_string_pretty(&status_json).unwrap()
                );
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }

        if let Some(completed) = final_payload {
            assert!(completed["deck_id"].is_string());
            assert!(completed["indexing"]["status"].is_string());
        } else {
            let last = last_status_payload.expect("job status payload must exist");
            assert!(
                last["data"]["Processing"]["progress"].is_string(),
                "expected processing progress when job is not yet completed: {}",
                serde_json::to_string_pretty(&last).unwrap()
            );
        }
    }

    // 2. Test Semantic Search
    let search_query = serde_json::json!({
        "query": "pitch deck",
        "limit": 3
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/decks/search")
                .header("Content-Type", common::APPLICATION_JSON)
                .body(Body::from(serde_json::to_vec(&search_query).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Search must return 200 (check Cohere API key and index)"
    );
    let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify Search Response Schema
    println!(
        "Search Response: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
    assert!(
        json["data"]["results"].is_array(),
        "response must include data.results array"
    );
    let results = json["data"]["results"].as_array().unwrap();
    if !results.is_empty() {
        assert!(
            results[0].get("score").and_then(Value::as_f64).is_some(),
            "each result must have score"
        );
    }
}
