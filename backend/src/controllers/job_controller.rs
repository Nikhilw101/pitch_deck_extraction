use crate::middleware::request_id::RequestId;
use crate::models::api_response::{ApiErrorResponse, ApiSuccessResponse};
use crate::services::job_service::{JobService, JobStatus};
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

pub async fn get_job_status(
    Extension(request_id): Extension<RequestId>,
    Extension(job_service): Extension<Arc<JobService>>,
    Path(job_id): Path<String>,
) -> Result<Json<ApiSuccessResponse<JobStatus>>, (StatusCode, Json<ApiErrorResponse>)> {
    let rid = request_id.0.clone();

    match job_service.get_job_status(&job_id) {
        Some(status) => Ok(Json(ApiSuccessResponse::ok(status, rid))),
        None => {
            let err = crate::errors::app_error::AppError::NotFound(format!("Job {} not found", job_id));
            Err((StatusCode::NOT_FOUND, Json(ApiErrorResponse::new(
                StatusCode::NOT_FOUND.as_u16().to_string(),
                err.to_string(),
                rid
            ))))
        }
    }
}
