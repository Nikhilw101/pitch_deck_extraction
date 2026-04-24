use axum::{extract::Request, middleware::Next, response::Response};
use chrono::Utc;
use std::time::Instant;
use tracing::info;

pub async fn log_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started_at = Instant::now();
    let timestamp = Utc::now().to_rfc3339();

    info!(
        timestamp = %timestamp,
        method = %method,
        endpoint = %path,
        "[INFO] Request received"
    );

    let response = next.run(request).await;
    let status = response.status().as_u16();
    let elapsed_ms = started_at.elapsed().as_millis();

    info!(
        timestamp = %Utc::now().to_rfc3339(),
        method = %method,
        endpoint = %path,
        status_code = status,
        duration_ms = elapsed_ms,
        "[SUCCESS] Response sent"
    );

    response
}
