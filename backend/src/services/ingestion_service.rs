use crate::errors::app_error::AppError;
use std::io::Write;
use tempfile::NamedTempFile;
use tracing::{error, info};

/// Create a temporary file used for upload streaming.
pub fn create_temp_file() -> Result<NamedTempFile, AppError> {
    NamedTempFile::new().map_err(|e| {
        error!("Failed to create temp file: {}", e);
        AppError::FileProcessingError(format!("Cannot create temp file: {}", e))
    })
}

/// Append a chunk of uploaded bytes to an existing temporary file.
pub fn append_chunk(temp_file: &mut NamedTempFile, chunk: &[u8]) -> Result<(), AppError> {
    temp_file.write_all(chunk).map_err(|e| {
        error!("Failed to write upload chunk to temp file: {}", e);
        AppError::FileProcessingError(format!("Cannot write file: {}", e))
    })
}

/// Save uploaded file to temporary location
pub async fn save_uploaded_file(data: &[u8], filename: &str) -> Result<NamedTempFile, AppError> {
    let mut temp_file = create_temp_file()?;
    append_chunk(&mut temp_file, data)?;

    info!("Saved uploaded file: {}", filename);
    Ok(temp_file)
}
