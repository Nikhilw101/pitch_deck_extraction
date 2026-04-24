pub mod deck_controller;
pub mod health_controller;
pub mod search_controller;
pub mod job_controller;

use crate::errors::app_error::AppError;
use crate::models::api_response::ApiErrorResponse;
use axum::{http::StatusCode, Json};

pub(crate) fn app_error_response(
    error: &AppError,
    request_id: String,
) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        error.status_code(),
        Json(ApiErrorResponse::new(
            error.error_code(),
            error.client_message(),
            request_id,
        )),
    )
}