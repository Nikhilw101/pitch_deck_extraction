use crate::models::api_response::ApiSuccessResponse;
use axum::Json;
use serde::Serialize;
use tracing::info;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub service: &'static str,
    pub status: &'static str,
}

pub async fn health_check() -> Json<ApiSuccessResponse<HealthResponse>> {
    info!("[INFO] API HIT: /api/health");
    Json(ApiSuccessResponse::with_message(
        HealthResponse {
            service: "pitch_deck_extractor",
            status: "ok",
        },
        "system-health-check".to_string(),
        "Service is healthy",
    ))
}
