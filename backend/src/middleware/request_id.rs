use axum::{extract::Request, middleware::Next, response::Response};
use uuid::Uuid;

/// Request-scoped ID for tracing and API responses (no PII)
#[derive(Clone)]
pub struct RequestId(pub String);

/// Injects RequestId into request extensions so handlers can return it in JSON.
pub async fn inject_request_id(mut request: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    tracing::info!(request_id = %request_id, "request started");
    next.run(request).await
}
