use chrono::Utc;
use serde::Serialize;

/// Standardized success response envelope
#[derive(Debug, Serialize)]
pub struct ApiSuccessResponse<T: Serialize> {
    pub status: &'static str,
    pub message: &'static str,
    pub data: T,
    pub request_id: String,
    pub timestamp: String,
}

impl<T: Serialize> ApiSuccessResponse<T> {
    pub fn ok(data: T, request_id: String) -> Self {
        Self::with_message(data, request_id, "Deck processed successfully")
    }

    pub fn with_message(data: T, request_id: String, message: &'static str) -> Self {
        Self {
            status: "success",
            message,
            data,
            request_id,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

/// Standardized error payload inside response
#[derive(Debug, Serialize)]
pub struct ApiErrorPayload {
    pub code: String,
    pub message: String,
}

/// Standardized error response envelope (no internal details or stack traces)
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub status: &'static str,
    pub error: ApiErrorPayload,
    pub request_id: String,
    pub timestamp: String,
}

impl ApiErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>, request_id: String) -> Self {
        Self {
            status: "error",
            error: ApiErrorPayload {
                code: code.into(),
                message: message.into(),
            },
            request_id,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}
