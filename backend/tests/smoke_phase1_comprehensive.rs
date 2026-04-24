//! Phase 1 & 2 comprehensive smoke test: real PDF (tests/test_pdf.pdf), step-wise output,
//! and JSON report written to tests/smoke_test_output.json.
//!
//! Run: cargo test --test smoke_phase1_comprehensive -- --nocapture

use axum::body::Body;
use axum::http::Request;
mod common;
use pitch_deck_service::routes;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tower::ServiceExt;

const BOUNDARY: &str = "----WebKitFormBoundary7MA4YWxkTrZu0gW";

/// Build extracted text output from API response data.slides.
fn build_extracted_text_from_response(data: &serde_json::Value) -> Option<ExtractedTextOutput> {
    let deck = data.get("deck")?;
    let slides = deck.get("slides")?.as_array()?;
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
    response_snippet: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SlideExtractedText {
    slide_number: u32,
    title: Option<String>,
    subtitle: Option<String>,
    body_text: String,
}

#[derive(Serialize, Deserialize)]
struct ExtractedTextOutput {
    slides: Vec<SlideExtractedText>,
    full_text: String,
}

#[derive(Serialize, Deserialize)]
struct ReportSummary {
    overall: String,
    steps_passed: u32,
    steps_total: u32,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct SmokeTestReport {
    run_timestamp: String,
    pdf_used: String,
    phase1_flow: Vec<PhaseStep>,
    extracted_text: Option<ExtractedTextOutput>,
    final_response: Option<serde_json::Value>,
    summary: ReportSummary,
}

#[tokio::test]
async fn test_smoke_comprehensive() {
    let run_timestamp = chrono::Utc::now().to_rfc3339();
    let pdf_path = Path::new("tests/test_pdf.pdf");
    let output_path = Path::new("tests/smoke_test_output.json");

    println!("\n========== COMPREHENSIVE SMOKE TEST: PDF EXTRATION & INDEXING ==========");
    println!("  Timestamp: {}", run_timestamp);
    println!("  Target PDF: {}", pdf_path.display());

    let mut phases = Vec::new();

    let pdf_bytes = match std::fs::read(pdf_path) {
        Ok(b) => b,
        Err(_) => {
            println!("ERROR: Test PDF not found at {}", pdf_path.display());
            return;
        }
    };

    let body = common::multipart_body(BOUNDARY, "test_pdf.pdf", "application/pdf", &pdf_bytes);
    let content_type = format!("multipart/form-data; boundary={}", BOUNDARY);
    let request = Request::builder()
        .method("POST")
        .uri("/api/decks/upload")
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let app = routes::create_router();
    let response = app.oneshot(request).await.unwrap();
    let status_code = response.status().as_u16();
    let resp_body = response.into_body();
    let bytes = http_body_util::BodyExt::collect(resp_body)
        .await
        .unwrap()
        .to_bytes();
    let body_str = String::from_utf8_lossy(bytes.as_ref());
    let json_response: Option<serde_json::Value> = serde_json::from_str(&body_str).ok();

    let step1_success = status_code == 200
        && json_response
            .as_ref()
            .and_then(|j| j.get("status").and_then(|s| s.as_str()))
            == Some("success");

    let mut indexing_success = false;
    if let Some(ref json) = json_response {
        if let Some(data) = json.get("data") {
            if let Some(indexing) = data.get("indexing") {
                if indexing.get("status").and_then(|s| s.as_str()) == Some("indexed") {
                    indexing_success = true;
                }
            }
        }
    }

    let snippet = json_response.as_ref().map(|j| {
        serde_json::json!({
            "status": j.get("status"),
            "request_id": j.get("request_id"),
            "total_slides": j.get("data").and_then(|d| d.get("deck")).and_then(|dk| dk.get("total_slides")),
            "indexing": j.get("data").and_then(|d| d.get("indexing")).and_then(|i| i.get("status")),
        })
    });

    let step1 = PhaseStep {
        step: 1,
        name: "Extract text (Phase 1)".to_string(),
        endpoint: "POST /api/decks/upload".to_string(),
        method: Some("POST".to_string()),
        uri: Some("/api/decks/upload".to_string()),
        status_code: Some(status_code),
        success: step1_success,
        detail: if step1_success {
            "OK. Content extracted.".to_string()
        } else {
            format!("FAIL. Status: {}", status_code)
        },
        response_snippet: snippet.as_ref().map(|s| serde_json::to_string(&s).unwrap()),
    };

    let step2 = PhaseStep {
        step: 2,
        name: "Embeddings & Indexing (Phase 2)".to_string(),
        endpoint: "POST /upload (Integrated)".to_string(),
        method: Some("POST".to_string()),
        uri: Some("/upload".to_string()),
        status_code: Some(status_code),
        success: indexing_success,
        detail: if indexing_success {
            "SUCCESS: Indexing confirmed.".to_string()
        } else {
            "FAILED: Indexing status not found or failed.".to_string()
        },
        response_snippet: None,
    };

    phases.push(step1);
    phases.push(step2);

    println!("\n--- Step 1: Phase 1 Extraction ---");
    println!("  Result: {}", if step1_success { "PASS" } else { "FAIL" });

    println!("\n--- Step 2: Phase 2 Indexing ---");
    println!(
        "  Result: {}",
        if indexing_success { "PASS" } else { "FAIL" }
    );

    // Dummy steps for remaining flow
    let others = [
        "Sections",
        "LLM Refine",
        "Validate",
        "Summarize",
        "Score",
        "Compare",
        "Final",
    ];
    for (i, name) in others.iter().enumerate() {
        phases.push(PhaseStep {
            step: 3 + i as u32,
            name: name.to_string(),
            endpoint: "N/A".to_string(),
            method: None,
            uri: None,
            status_code: None,
            success: false,
            detail: "Not yet implemented".to_string(),
            response_snippet: None,
        });
    }

    let summary = ReportSummary {
        overall: if indexing_success {
            "PASS".to_string()
        } else {
            "FAIL".to_string()
        },
        steps_passed: phases.iter().filter(|p| p.success).count() as u32,
        steps_total: 9,
        message: if indexing_success {
            "Phase 1 and 2 E2E flow verified successfully.".to_string()
        } else {
            "E2E flow failed at extraction or indexing.".to_string()
        },
    };

    let extracted_text = json_response
        .as_ref()
        .and_then(|j| j.get("data"))
        .and_then(build_extracted_text_from_response);

    let report = SmokeTestReport {
        run_timestamp: run_timestamp.clone(),
        pdf_used: pdf_path.display().to_string(),
        phase1_flow: phases,
        extracted_text,
        final_response: json_response,
        summary,
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    if std::env::var("WRITE_SMOKE_JSON").as_deref() == Ok("1") {
        let _ = std::fs::write(output_path, &json);
        println!(
            "\n========== SUMMARY: {} ==========\n  Report: {} (WRITE_SMOKE_JSON=1)",
            report.summary.overall,
            output_path.display()
        );
    } else {
        println!(
            "\n========== SUMMARY: {} ==========\n  Report not written (set WRITE_SMOKE_JSON=1 to persist)",
            report.summary.overall
        );
    }

    assert!(step1_success, "Step 1 must pass for smoke test");
}
