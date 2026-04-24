use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),

    #[error("File processing error: {0}")]
    FileProcessingError(String),

    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),

    #[error("Extraction error: {0}")]
    ExtractionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid multipart data")]
    InvalidMultipart,

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Not Found: {0}")]
    NotFound(String),
}

/// Error code for API responses (no internal details)
impl AppError {
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::UnsupportedFileType(_) => "UNSUPPORTED_FILE_TYPE",
            AppError::InvalidMultipart => "INVALID_MULTIPART",
            AppError::FileProcessingError(_) => "FILE_PROCESSING_ERROR",
            AppError::ExtractionError(_) => "EXTRACTION_ERROR",
            AppError::ValidationError(_) => "VALIDATION_ERROR",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InternalServerError(_) | AppError::IoError(_) => "INTERNAL_ERROR",
        }
    }

    /// Safe message for API response (never expose internal paths or stack traces)
    pub fn client_message(&self) -> String {
        match self {
            AppError::UnsupportedFileType(filename) => {
                format!(
                    "Unsupported file type. Only PDF and PPTX are allowed. Received: {}",
                    filename
                )
            }
            AppError::InvalidMultipart => {
                "Invalid file upload: missing or malformed multipart data.".to_string()
            }
            AppError::FileProcessingError(_) => {
                "File processing failed. Check file is not corrupted and try again.".to_string()
            }
            AppError::ExtractionError(_) => {
                "Document extraction failed. The file may be corrupted or in an unsupported format."
                    .to_string()
            }
            AppError::ValidationError(message) => message.clone(),
            AppError::NotFound(message) => message.clone(),
            AppError::InternalServerError(_) | AppError::IoError(_) => {
                "An internal error occurred. Please try again later.".to_string()
            }
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::UnsupportedFileType(_) | AppError::InvalidMultipart => {
                StatusCode::BAD_REQUEST
            }
            AppError::ValidationError(_) => StatusCode::BAD_REQUEST,
            AppError::FileProcessingError(_) | AppError::ExtractionError(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(serde_json::json!({
            "error": self.client_message()
        }));
        (status, body).into_response()
    }
}
