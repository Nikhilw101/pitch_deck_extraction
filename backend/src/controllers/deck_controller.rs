use crate::errors::app_error::AppError;
use crate::middleware::request_id::RequestId;
use crate::models::api_response::{ApiErrorResponse, ApiSuccessResponse};
use crate::models::deck_model::{FileType, IndexingMetadata, ProcessingResponse, JobIdResponse};
use crate::services::{
    embedding_service::EmbeddingService, extraction_service, ingestion_service,
    llm_service::LlmService, pipeline_service::PipelineService,
    job_service::JobService,
};
use crate::controllers::app_error_response;
use axum::{
    extract::{Extension, Multipart},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::{error, info};

/// Result type for upload: success envelope or standardized error envelope with request_id
pub type UploadResponse =
    Result<Json<ApiSuccessResponse<JobIdResponse>>, (StatusCode, Json<ApiErrorResponse>)>;

/// Handle deck upload and processing (Phase 1 + Phase 2 + Phase 3 + Phase 4).
///
/// This endpoint accepts PDF or PPTX files, extracts their content, generates embeddings,
/// indexes them, classifies sections, and processes through LLM for structured output.
///
/// # Process
/// 1. Receives multipart file upload
/// 2. Validates file type (PDF or PPTX only)
/// 3. Extracts text and structure (Phase 1)
/// 4. Generates embeddings and indexes vectors (Phase 2)
/// 5. Classifies slides into sections (Phase 3)
/// 6. LLM processing: structured extraction, validation, summaries, signals (Phase 4)
/// 7. Returns complete processed deck with all analysis
///
/// # Arguments
/// * `request_id` - Request ID for tracing (injected by middleware)
/// * `embedding_service` - Service for generating embeddings
/// * `pipeline_service` - Service for processing decks
/// * `llm_service` - LLM service for Phase 4 processing
/// * `multipart` - Multipart form data containing the file
///
/// # Returns
/// * `Ok(Json<ApiSuccessResponse<ProcessingResponse>>)` - Successfully processed deck
/// * `Err((StatusCode, Json<ApiErrorResponse>))` - Error with appropriate status code
///
/// # Errors
/// - `400 BAD_REQUEST` - Unsupported file type or invalid multipart data
/// - `422 UNPROCESSABLE_ENTITY` - File processing or extraction failed
/// - `500 INTERNAL_SERVER_ERROR` - Internal server error
pub async fn upload_deck(
    Extension(request_id): Extension<RequestId>,
    Extension(embedding_service): Extension<Arc<dyn EmbeddingService>>,
    Extension(pipeline_service): Extension<Arc<PipelineService>>,
    Extension(llm_service): Extension<Arc<dyn LlmService>>,
    Extension(job_service): Extension<Arc<JobService>>,
    multipart: Multipart,
) -> UploadResponse {
    let mut multipart = multipart;
    let rid = request_id.0.clone();
    info!("[INFO] API HIT: /api/decks/upload");
    info!(request_id = %rid, "Received deck upload request");

    let field = multipart
        .next_field()
        .await
        .map_err(|_| AppError::InvalidMultipart)
        .and_then(|f| f.ok_or(AppError::InvalidMultipart));

    let mut field = match field {
        Ok(f) => f,
        Err(e) => {
            error!(request_id = %rid, "Invalid multipart: {}", e);
            return Err(app_error_response(&e, rid));
        }
    };

    let filename = field
        .file_name()
        .ok_or(AppError::InvalidMultipart)
        .map(|s| s.to_string());

    let filename = match filename {
        Ok(f) => f,
        Err(e) => {
            error!(request_id = %rid, "Missing filename");
            return Err(app_error_response(&e, rid));
        }
    };

    // Stream multipart data directly into a temp file to avoid buffering full uploads in memory.
    let mut temp_file = match ingestion_service::create_temp_file() {
        Ok(t) => t,
        Err(e) => {
            error!(request_id = %rid, "Temp file creation failed: {}", e);
            return Err(app_error_response(&e, rid));
        }
    };

    let mut total_size = 0usize;
    loop {
        let chunk = match field.chunk().await {
            Ok(c) => c,
            Err(e) => {
                error!(request_id = %rid, "Failed to read upload chunk: {}", e);
                let err = AppError::FileProcessingError(e.to_string());
                return Err(app_error_response(&err, rid));
            }
        };

        let Some(chunk) = chunk else {
            break;
        };

        total_size += chunk.len();
        if let Err(e) = ingestion_service::append_chunk(&mut temp_file, &chunk) {
            error!(request_id = %rid, "Failed writing upload chunk: {}", e);
            return Err(app_error_response(&e, rid));
        }
    }

    info!(request_id = %rid, file = %filename, size = total_size, "Processing file");

    let file_type = match FileType::from_filename(&filename) {
        Some(ft) => ft,
        None => {
            let err = AppError::UnsupportedFileType(filename.clone());
            error!(request_id = %rid, filename = %filename, "Unsupported file type");
            return Err(app_error_response(&err, rid));
        }
    };

    let temp_path = temp_file.path().to_path_buf();
    let job_id = job_service.create_job();
    let rid_clone = rid.clone();
    let filename_clone = filename.clone();
    let job_service_clone = job_service.clone();
    let embedding_service_clone = embedding_service.clone();
    let pipeline_service_clone = pipeline_service.clone();
    let llm_service_clone = llm_service.clone();

    let job_id_clone = job_id.clone();
    // Spawn background task
    tokio::spawn(async move {
        // Move temp_file into closure so it isn't deleted until extraction finishes
        let _temp_holder = temp_file;
        job_service_clone.update_progress(&job_id_clone, "Extracting PDF/PPTX content");
        
        let extracted_deck = match file_type {
            FileType::Pdf => extraction_service::extract_pdf(&temp_path, &filename_clone).await,
            FileType::Pptx => extraction_service::extract_pptx(&temp_path, &filename_clone).await,
        };

        let extracted_deck = match extracted_deck {
            Ok(deck) => deck,
            Err(e) => {
                error!(request_id = %rid_clone, "Extraction failed: {}", e);
                job_service_clone.fail_job(&job_id_clone, format!("Extraction failed: {}", e));
                return;
            }
        };

        job_service_clone.update_progress(&job_id_clone, "Running pipeline (embeddings, sections, LLM)");
        
        let pipeline_result = pipeline_service_clone
            .run_full_pipeline(&extracted_deck, llm_service_clone)
            .await;

        let response = ProcessingResponse {
            deck_id: extracted_deck.deck_id,
            filename: extracted_deck.filename,
            file_type: extracted_deck.file_type,
            total_slides: extracted_deck.total_slides,
            metadata: extracted_deck.metadata,
            indexing: IndexingMetadata {
                slides_indexed: extracted_deck.slides.len(),
                embedding_dimension: embedding_service_clone.get_dimension(),
                status: pipeline_result.indexing_status,
            },
            grouped_deck: pipeline_result.grouped_deck,
            structured_output: pipeline_result.structured_output,
        };

        job_service_clone.complete_job(&job_id_clone, response);
        info!(request_id = %rid_clone, "Job completed: {}", job_id_clone);
    });

    Ok(Json(ApiSuccessResponse::ok(
        JobIdResponse {
            job_id,
            status: "processing".to_string(),
        },
        rid
    )))
}
