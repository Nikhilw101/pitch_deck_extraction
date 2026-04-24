use crate::middleware::request_id::RequestId;
use crate::models::api_response::{ApiErrorResponse, ApiSuccessResponse};
use crate::models::vector_model::{SearchRequest, SearchResponse};
use crate::services::embedding_service::EmbeddingService;
use crate::services::vector_store_service::VectorStore;
use crate::{controllers::app_error_response, errors::app_error::AppError};
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use std::sync::Arc;
use tracing::{error, info};

/// Handle semantic search across indexed pitch decks.
///
/// Performs vector similarity search to find slides that are semantically similar
/// to the query text.
///
/// # Process
/// 1. Generates embedding vector for the query text
/// 2. Searches vector store for k nearest neighbors
/// 3. Returns matching slides with similarity scores
///
/// # Arguments
/// * `request_id` - Request ID for tracing (injected by middleware)
/// * `embedding_service` - Service for generating query embeddings
/// * `vector_store` - Vector database for similarity search
/// * `payload` - Search request containing query text and result limit
///
/// # Returns
/// * `Ok(Json<ApiSuccessResponse<SearchResponse>>)` - Search results
/// * `Err((StatusCode, Json<ApiErrorResponse>))` - Error with appropriate status code
///
/// # Errors
/// - `500 INTERNAL_SERVER_ERROR` - Embedding generation or search failed
pub async fn search_decks(
    Extension(request_id): Extension<RequestId>,
    Extension(embedding_service): Extension<Arc<dyn EmbeddingService>>,
    Extension(vector_store): Extension<Arc<VectorStore>>,
    Json(payload): Json<SearchRequest>,
) -> Result<Json<ApiSuccessResponse<SearchResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let rid = request_id.0.clone();
    info!("[INFO] API HIT: /api/decks/search");
    let query = payload.query.trim();
    if query.is_empty() {
        let err = AppError::ValidationError("query must not be empty".to_string());
        return Err(app_error_response(&err, rid));
    }
    if payload.limit == 0 || payload.limit > 20 {
        let err = AppError::ValidationError("limit must be between 1 and 20".to_string());
        return Err(app_error_response(&err, rid));
    }

    info!(request_id = %rid, query = %query, "Received search request");

    // 1. Generate query embedding (using search_query for better accuracy)
    let query_vector = match embedding_service
        .generate_embeddings(vec![query.to_string()], "search_query")
        .await
    {
        Ok(vectors) => {
            if vectors.is_empty() {
                let err = AppError::InternalServerError(
                    "Failed to generate query embedding".to_string(),
                );
                return Err((
                    err.status_code(),
                    Json(ApiErrorResponse::new(err.error_code(), err.client_message(), rid)),
                ));
            }
            vectors[0].clone()
        }
        Err(e) => {
            error!(request_id = %rid, "Query embedding failed: {}", e);
            return Err(app_error_response(&e, rid));
        }
    };

    // 2. Search in HNSW
    let results = match vector_store.search(&query_vector, payload.limit).await {
        Ok(res) => res,
        Err(e) => {
            error!(request_id = %rid, "Vector search failed: {}", e);
            return Err(app_error_response(&e, rid));
        }
    };

    info!(request_id = %rid, results = results.len(), "Search completed");
    Ok(Json(ApiSuccessResponse::with_message(
        SearchResponse { results },
        rid,
        "Search completed successfully",
    )))
}
