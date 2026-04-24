//! Phase 1 smoke test for PPTX: uses tests/test.pptx, same JSON output as PDF test.
//! Output: tests/smoke_test_output_pptx.json (extracted_text, final_response, etc.)
//!
//! Place a .pptx file at tests/test.pptx then run:
//!   cargo test --test smoke_phase1_pptx -- --nocapture

use axum::body::Body;
use axum::http::{Request, StatusCode};
mod common;
use pitch_deck_service::routes;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tower::ServiceExt;

const BOUNDARY: &str = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
const PPTX_FILENAME: &str = "test.pptx";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PhaseStep {
    step: u32,
    name: String,
    endpoint: String,
    method: Option<String>,
    uri: Option<String>,
    status_code: Option<u16>,
    success: bool,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_snippet: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlideExtractedText {
    slide_number: u32,
    title: Option<String>,
    subtitle: Option<String>,
    body_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedTextOutput {
    slides: Vec<SlideExtractedText>,
    full_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SmokeTestReport {
    run_timestamp: String,
    file_used: String,
    phase1_flow: Vec<PhaseStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extracted_text: Option<ExtractedTextOutput>,
    final_response: Option<serde_json::Value>,
    summary: ReportSummary,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReportSummary {
    overall: String,
    steps_passed: u32,
    steps_total: u32,
    message: String,
}

fn build_extracted_text_from_response(data: &serde_json::Value) -> Option<ExtractedTextOutput> {
    let slides = data.get("slides")?.as_array()?;
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
        });
    }

    let full_text = full_parts.join("\n\n");
    Some(ExtractedTextOutput {
        slides: out_slides,
        full_text,
    })
}


#[tokio::test]
async fn phase1_pptx_smoke_test() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pptx_path = manifest_dir.join("tests").join(PPTX_FILENAME);
    let output_path = manifest_dir
        .join("tests")
        .join("smoke_test_output_pptx.json");

    let run_timestamp = chrono::Utc::now().to_rfc3339();
    let mut phases = Vec::new();

    println!("\n========== PHASE 1 PPTX SMOKE TEST ==========");
    println!("PPTX: {}", pptx_path.display());
    println!("Output: {}", output_path.display());
    println!("Run at: {}\n", run_timestamp);

    let pptx_bytes = match tokio::fs::read(&pptx_path).await {
        Ok(b) => b,
        Err(e) => {
            println!(
                "  SKIP: {} not found ({}). Put a .pptx file at tests/{}",
                pptx_path.display(),
                e,
                PPTX_FILENAME
            );
            let report = SmokeTestReport {
                run_timestamp: run_timestamp.clone(),
                file_used: pptx_path.display().to_string(),
                phase1_flow: vec![PhaseStep {
                    step: 1,
                    name: "Extract text (PPTX)".to_string(),
                    endpoint: "POST /api/decks/upload".to_string(),
                    method: Some("POST".to_string()),
                    uri: Some("/api/decks/upload".to_string()),
                    status_code: None,
                    success: false,
                    detail: format!("Missing input file: {}", e),
                    response_snippet: None,
                }],
                extracted_text: None,
                final_response: None,
                summary: ReportSummary {
                    overall: "SKIP".to_string(),
                    steps_passed: 0,
                    steps_total: 1,
                    message: format!(
                        "No tests/{} found. Add a PPTX file to run this test.",
                        PPTX_FILENAME
                    ),
                },
            };
            if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
                let _ =
                    std::fs::write(&output_path, serde_json::to_string_pretty(&report).unwrap());
                println!(
                    "  JSON output: {} (WRITE_SMOKE_JSON=1)",
                    output_path.display()
                );
            } else {
                println!(
                    "  JSON output not written (set WRITE_SMOKE_JSON=1 to persist at {})",
                    output_path.display()
                );
            }
            return;
        }
    };

    println!("--- Step 1: Extract text (PPTX) ---");
    println!("  Endpoint: POST /api/decks/upload");

    let body = common::multipart_body(
        BOUNDARY,
        PPTX_FILENAME,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        &pptx_bytes,
    );
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);
    let request = Request::builder()
        .method("POST")
        .uri("/api/decks/upload")
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = routes::create_router();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let status_code = status.as_u16();
    let resp_body = response.into_body();
    let bytes = http_body_util::BodyExt::collect(resp_body)
        .await
        .unwrap()
        .to_bytes();
    let body_str = String::from_utf8_lossy(bytes.as_ref());
    let json_response: Option<serde_json::Value> = serde_json::from_str(&body_str).ok();

    let step1_success = status == StatusCode::OK
        && json_response
            .as_ref()
            .and_then(|j| j.get("status").and_then(|s| s.as_str()))
            == Some("success")
        && json_response.as_ref().and_then(|j| j.get("data")).is_some();

    let snippet = json_response.as_ref().map(|j| {
        serde_json::json!({
            "status": j.get("status"),
            "message": j.get("message"),
            "request_id": j.get("request_id"),
            "slide_count": j.get("data").and_then(|d| d.get("slides")).and_then(|s| s.as_array()).map(|a| a.len()),
        })
    });

    phases.push(PhaseStep {
        step: 1,
        name: "Extract text (PPTX)".to_string(),
        endpoint: "POST /api/decks/upload".to_string(),
        method: Some("POST".to_string()),
        uri: Some("/api/decks/upload".to_string()),
        status_code: Some(status_code),
        success: step1_success,
        detail: if step1_success {
            format!(
                "OK. status={}, slides={}",
                status_code,
                snippet
                    .as_ref()
                    .and_then(|s| s.get("slide_count").and_then(|c| c.as_u64()))
                    .unwrap_or(0)
            )
        } else {
            format!(
                "FAIL. status={}, body_preview={}",
                status_code,
                body_str.chars().take(200).collect::<String>()
            )
        },
        response_snippet: snippet,
    });

    println!("  Status: {}", status_code);
    println!("  Result: {}", if step1_success { "PASS" } else { "FAIL" });
    println!("  Detail: {}", phases.last().unwrap().detail);
    if let Some(ref j) = json_response {
        if let Some(rid) = j.get("request_id").and_then(|r| r.as_str()) {
            println!("  request_id: {}", rid);
        }
        if let Some(n) = j
            .get("data")
            .and_then(|d| d.get("total_slides"))
            .and_then(|t| t.as_u64())
        {
            println!("  total_slides: {}", n);
        }
    }

    let overall = if step1_success { "PASS" } else { "FAIL" };
    let summary = ReportSummary {
        overall: overall.to_string(),
        steps_passed: if step1_success { 1 } else { 0 },
        steps_total: 1,
        message: if step1_success {
            "PPTX extraction completed. Extracted text in extracted_text.".to_string()
        } else {
            "PPTX extraction failed.".to_string()
        },
    };

    let extracted_text = json_response
        .as_ref()
        .and_then(|j| j.get("data"))
        .and_then(build_extracted_text_from_response);

    let report = SmokeTestReport {
        run_timestamp: run_timestamp.clone(),
        file_used: pptx_path.display().to_string(),
        phase1_flow: phases,
        extracted_text,
        final_response: json_response,
        summary,
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
        let write_ok = std::fs::write(&output_path, &json).is_ok();
        let out_path_str = output_path.display().to_string();

        println!(
            "\n========== SUMMARY: {} ==========\n  Steps passed: {} / {}\n  {}\n  JSON output: {}",
            report.summary.overall,
            report.summary.steps_passed,
            report.summary.steps_total,
            report.summary.message,
            if write_ok {
                format!("{} (WRITE_SMOKE_JSON=1)", out_path_str)
            } else {
                "WRITE FAILED".to_string()
            }
        );
        if !write_ok {
            eprintln!(
                "  WARNING: Could not write report to {}",
                output_path.display()
            );
        }
    } else {
        println!(
            "\n========== SUMMARY: {} ==========\n  Steps passed: {} / {}\n  {}\n  JSON output not written (set WRITE_SMOKE_JSON=1 to persist at {})",
            report.summary.overall,
            report.summary.steps_passed,
            report.summary.steps_total,
            report.summary.message,
            output_path.display()
        );
    }

    assert!(
        step1_success,
        "PPTX extraction must pass when {} is present",
        pptx_path.display()
    );
}
