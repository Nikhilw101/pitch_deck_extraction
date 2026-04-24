//! Smoke tests for POST /api/decks/upload using tests/test_pdf.pdf.
//! Validates: endpoint reachable, valid PDF, invalid file type, corrupted PDF,
//! missing/invalid multipart. Requires COHERE_API_KEY and Ollama for full pipeline.
//! Run with: cargo test --test smoke_test_upload -- --nocapture

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Extension;
use http_body_util::BodyExt;
mod common;
use pitch_deck_service::config;
use pitch_deck_service::routes;
use pitch_deck_service::services::embedding_service::{CohereClient, EmbeddingService};
use pitch_deck_service::services::llm_service::{LlmService, OllamaClient};
use pitch_deck_service::services::pipeline_service::PipelineService;
use pitch_deck_service::services::vector_store_service::VectorStore;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

const UPLOAD_URI: &str = "/api/decks/upload";
const BOUNDARY: &str = "----WebKitFormBoundary7MA4YWxkTrZu0gW";

fn app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = config::load().expect("Config load failed. Set COHERE_API_KEY and .env");
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
            "tests/tmp_smoke_upload_index".to_string(),
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

    routes::create_router()
        .layer(Extension(embedding_service as Arc<dyn EmbeddingService>))
        .layer(Extension(vector_store))
        .layer(Extension(pipeline_service))
        .layer(Extension(llm_service as Arc<dyn LlmService>))
}

async fn read_test_pdf() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_pdf.pdf");
    tokio::fs::read(&path)
        .await
        .unwrap_or_else(|e| panic!("Missing tests/test_pdf.pdf: {}", e))
}

fn parse_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|e| {
        panic!(
            "Invalid JSON response: {}; body: {}",
            e,
            String::from_utf8_lossy(body)
        )
    })
}

#[tokio::test]
async fn smoke_upload_valid_pdf_returns_200_and_structured_success() {
    let test_start = std::time::Instant::now();
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let pdf = read_test_pdf().await;
    let body = common::multipart_body(BOUNDARY, "test_pdf.pdf", "application/pdf", &pdf);
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);

    let request = Request::builder()
        .method("POST")
        .uri(UPLOAD_URI)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = app();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid PDF must return 200"
    );

    let body = response.into_body();
    let bytes = BodyExt::collect(body).await.unwrap().to_bytes();
    let json = parse_json(bytes.as_ref());

    assert_eq!(json.get("status").and_then(Value::as_str), Some("success"));
    assert_eq!(
        json.get("message").and_then(Value::as_str),
        Some("Deck processed successfully")
    );
    assert!(json.get("data").is_some(), "Success must include data");
    assert!(
        json.get("request_id")
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            == Some(true)
    );
    assert!(
        json.get("timestamp")
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            == Some(true)
    );

    let data = json.get("data").unwrap();
    assert!(data.get("deck_id").is_some());
    assert_eq!(
        data.get("filename").and_then(Value::as_str),
        Some("test_pdf.pdf")
    );
    assert!(data.get("metadata").is_some());
    assert!(data.get("indexing").is_some());
    assert_eq!(
        data.get("indexing")
            .and_then(|i| i.get("status"))
            .and_then(Value::as_str),
        Some("indexed")
    );

    let elapsed = test_start.elapsed();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    println!();
    println!("════════════════════════════════════════════════════════════");
    println!("  JSON: data.structured_output in response (no file written)");
    println!(
        "  Vector index: {}",
        manifest
            .join("tests")
            .join("tmp_smoke_upload_index.meta.json")
            .display()
    );
    println!(
        "  TOTAL TIME: {:.2} s ({:.2} min)",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / 60.0
    );
    println!("════════════════════════════════════════════════════════════");
}

#[tokio::test]
async fn smoke_upload_unsupported_file_type_returns_400_and_structured_error() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let body = common::multipart_body(BOUNDARY, "script.exe", "application/octet-stream", b"fake");
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);

    let request = Request::builder()
        .method("POST")
        .uri(UPLOAD_URI)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = app();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Unsupported file type must return 400"
    );

    let body = response.into_body();
    let bytes = BodyExt::collect(body).await.unwrap().to_bytes();
    let json = parse_json(bytes.as_ref());

    assert_eq!(json.get("status").and_then(Value::as_str), Some("error"));
    let err = json.get("error").expect("error payload");
    assert!(err.get("code").and_then(Value::as_str).is_some());
    assert!(
        err.get("message")
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            == Some(true)
    );
    assert!(
        json.get("request_id")
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            == Some(true)
    );
    assert!(
        json.get("timestamp")
            .and_then(Value::as_str)
            .map(|s| !s.is_empty())
            == Some(true)
    );
    assert!(
        json.get("stack_trace").is_none(),
        "Must not leak internal details"
    );
}

#[tokio::test]
async fn smoke_upload_corrupted_pdf_returns_error_and_structured_response() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let body = common::multipart_body(
        BOUNDARY,
        "corrupted.pdf",
        "application/pdf",
        b"not a real pdf content \x00\x01\x02",
    );
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);

    let request = Request::builder()
        .method("POST")
        .uri(UPLOAD_URI)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = app();
    let response = app.oneshot(request).await.unwrap();

    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Corrupted PDF should return 422 or 500"
    );

    let body = response.into_body();
    let bytes = BodyExt::collect(body).await.unwrap().to_bytes();
    let json = parse_json(bytes.as_ref());

    assert_eq!(json.get("status").and_then(Value::as_str), Some("error"));
    assert!(json.get("error").is_some());
    assert!(json.get("request_id").is_some());
    assert!(json.get("stack_trace").is_none());
}

#[tokio::test]
async fn smoke_upload_invalid_multipart_missing_file_returns_400() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Sending JSON instead of multipart: extractor may fail before our handler (no request_id in body)
    let request = Request::builder()
        .method("POST")
        .uri(UPLOAD_URI)
        .header("content-type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();

    let app = app();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body();
    let bytes = BodyExt::collect(body).await.unwrap().to_bytes();
    let raw = String::from_utf8_lossy(bytes.as_ref());

    // Framework may return plain text when multipart parsing fails; must not leak internals
    if let Ok(json) = serde_json::from_slice::<Value>(bytes.as_ref()) {
        assert_eq!(json.get("status").and_then(Value::as_str), Some("error"));
        assert!(json.get("error").is_some());
        assert!(json.get("request_id").is_some());
    }
    assert!(
        !raw.contains("panic") && !raw.contains(" at 0x") && !raw.contains("stack backtrace"),
        "Must not expose stack traces or panic info"
    );
}

#[tokio::test]
async fn smoke_upload_txt_extension_returns_400() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let body = common::multipart_body(BOUNDARY, "readme.txt", "text/plain", b"some text");
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);

    let request = Request::builder()
        .method("POST")
        .uri(UPLOAD_URI)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = app();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body();
    let bytes = BodyExt::collect(body).await.unwrap().to_bytes();
    let json = parse_json(bytes.as_ref());
    assert_eq!(json.get("status").and_then(Value::as_str), Some("error"));
    let code = json
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(Value::as_str);
    assert_eq!(code, Some("UNSUPPORTED_FILE_TYPE"));
}
