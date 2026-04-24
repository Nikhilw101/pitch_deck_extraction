use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Serialize)]
struct SmokeInput {
    file_name: String,
    file_path: String,
    file_size_bytes: u64,
    endpoint: String,
}

#[derive(Debug, Serialize)]
struct SmokeOutput {
    timestamp: String,
    status: String,
    input: SmokeInput,
    http_status: u16,
    quality_passed: bool,
    placeholder_string_count: usize,
    api_response: Value,
    extracted_data: Value,
}

fn count_placeholder_strings(value: &Value) -> usize {
    match value {
        Value::String(s) => usize::from(s.trim().eq_ignore_ascii_case("string")),
        Value::Array(arr) => arr.iter().map(count_placeholder_strings).sum(),
        Value::Object(map) => map.values().map(count_placeholder_strings).sum(),
        _ => 0,
    }
}

#[tokio::test]
#[ignore = "requires running backend + external dependencies (Cohere/Ollama)"]
async fn smoke_live_upload_writes_json_output() {
    let base_url =
        std::env::var("SMOKE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
    let endpoint = format!("{}/api/decks/upload", base_url.trim_end_matches('/'));
    let pdf_path = Path::new("tests/test_pdf.pdf");

    println!("== Smoke test started ==");
    println!("Request target: {}", endpoint);
    println!("Processing file: {}", pdf_path.display());

    let file_bytes = tokio::fs::read(pdf_path)
        .await
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", pdf_path.display(), e));
    let file_size = file_bytes.len() as u64;
    let file_name = "test_pdf.pdf".to_string();

    println!("Request sent");
    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name.clone())
        .mime_str("application/pdf")
        .expect("valid mime type");
    let form = reqwest::multipart::Form::new().part("file", part);
    let response = reqwest::Client::new()
        .post(&endpoint)
        .multipart(form)
        .send()
        .await
        .expect("Request failed");

    println!("Processing stage complete, parsing response");
    let status = response.status().as_u16();
    let api_response: Value = response
        .json()
        .await
        .unwrap_or_else(|e| panic!("Invalid JSON response: {}", e));

    let success = status == 200
        && api_response
            .get("status")
            .and_then(Value::as_str)
            .map(|s| s == "success")
            .unwrap_or(false);

    let extracted_data = api_response.get("data").cloned().unwrap_or(Value::Null);
    let placeholder_string_count = count_placeholder_strings(&extracted_data);
    let allow_placeholders = std::env::var("SMOKE_ALLOW_PLACEHOLDERS").as_deref() == Ok("1");
    let quality_passed = placeholder_string_count == 0 || allow_placeholders;

    let result = SmokeOutput {
        timestamp: chrono::Utc::now().to_rfc3339(),
        status: if success {
            "success".to_string()
        } else {
            "failure".to_string()
        },
        input: SmokeInput {
            file_name,
            file_path: pdf_path.display().to_string(),
            file_size_bytes: file_size,
            endpoint,
        },
        http_status: status,
        quality_passed,
        placeholder_string_count,
        api_response,
        extracted_data,
    };

    tokio::fs::create_dir_all("outputs")
        .await
        .expect("Failed to create outputs directory");
    let output_path = format!(
        "outputs/smoke_output_{}.json",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    tokio::fs::write(
        &output_path,
        serde_json::to_vec_pretty(&result).expect("Failed to serialize output"),
    )
    .await
    .expect("Failed to write output JSON");

    println!("Response received (HTTP {})", status);
    println!("File saved: {}", output_path);
    println!(
        "Quality check: placeholder_string_count={}, quality_passed={}",
        placeholder_string_count, quality_passed
    );

    assert!(success, "Smoke test failed. Check output JSON for details.");
    assert!(
        quality_passed,
        "Smoke quality check failed: found {} placeholder 'string' values in extracted_data. Set SMOKE_ALLOW_PLACEHOLDERS=1 to bypass.",
        placeholder_string_count
    );
}
