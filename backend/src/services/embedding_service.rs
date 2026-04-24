use crate::errors::app_error::AppError;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;
use tracing::{error, info};

/// Maximum number of texts per Cohere API request (free tier limit)
const COHERE_MAX_TEXTS_PER_REQUEST: usize = 96;

/// Cohere API endpoint for embeddings
const COHERE_EMBED_API_URL: &str = "https://api.cohere.ai/v1/embed";

/// HTTP status code for rate limiting
const HTTP_STATUS_RATE_LIMIT: u16 = 429;

/// Trait for generating text embeddings for semantic search.
///
/// Implementations convert text strings into dense vector representations
/// that can be used for similarity search and retrieval.
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Generate embeddings for a list of text strings.
    ///
    /// # Arguments
    /// * `texts` - Vector of text strings to embed
    /// * `input_type` - Type of input for embedding model:
    ///   - `"search_document"` - For indexing documents (better for storage)
    ///   - `"search_query"` - For search queries (better for retrieval)
    ///
    /// # Returns
    /// * `Ok(Vec<Vec<f32>>)` - Vector of embedding vectors, one per input text
    /// * `Err(AppError)` - Error if embedding generation fails
    async fn generate_embeddings(
        &self,
        texts: Vec<String>,
        input_type: &str,
    ) -> Result<Vec<Vec<f32>>, AppError>;

    /// Get the dimension (size) of embedding vectors produced by this service.
    ///
    /// # Returns
    /// The number of dimensions in each embedding vector (e.g., 768, 1024)
    fn get_dimension(&self) -> usize;
}

/// Cohere API client for generating text embeddings.
///
/// Uses Cohere's embedding API to convert text into dense vector representations.
/// Supports chunking for API rate limits and automatic retries with exponential backoff.
pub struct CohereClient {
    client: Client,
    api_key: String,
    model: String,
    dimension: usize,
}

impl CohereClient {
    /// Create a new Cohere embedding client.
    ///
    /// # Arguments
    /// * `api_key` - Cohere API key for authentication
    /// * `model` - Model name (e.g., "embed-english-v3.0")
    ///
    /// # Returns
    /// * `Ok(CohereClient)` - Successfully created client
    /// * `Err(AppError)` - Error if HTTP client creation fails
    ///
    /// # Note
    /// The embedding dimension is automatically determined based on the model name
    /// (v3 models use 1024 dimensions, others use 768).
    pub fn new(api_key: String, model: String) -> Result<Self, AppError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AppError::InternalServerError(format!("Failed to build HTTP client: {}", e))
            })?;

        // Mapping model names to dimensions (could be expanded)
        let dimension = if model.contains("v3") { 1024 } else { 768 };

        Ok(Self {
            client,
            api_key,
            model,
            dimension,
        })
    }
}

#[derive(Serialize)]
struct CohereEmbedRequest {
    texts: Vec<String>,
    model: String,
    input_type: String, // Cohere v3 requires input_type: "search_document" or "search_query"
}

#[derive(Deserialize)]
struct CohereEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[async_trait]
impl EmbeddingService for CohereClient {
    async fn generate_embeddings(
        &self,
        texts: Vec<String>,
        input_type: &str,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        info!(
            "Generating embeddings for {} texts using Cohere ({})",
            texts.len(),
            input_type
        );

        // Chunk texts to respect Cohere API limits
        let chunks = texts.chunks(COHERE_MAX_TEXTS_PER_REQUEST);
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in chunks {
            let request_body = CohereEmbedRequest {
                texts: chunk.to_vec(),
                model: self.model.clone(),
                input_type: input_type.to_string(),
            };

            let retry_strategy = ExponentialBackoff::from_millis(1000).map(jitter).take(3);

            let response = Retry::spawn(retry_strategy, || async {
                self.client
                    .post(COHERE_EMBED_API_URL)
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .json(&request_body)
                    .send()
                    .await
            })
            .await
            .map_err(|e| {
                error!("Cohere API request failed after retries: {}", e);
                AppError::InternalServerError(format!("Cohere API failed: {}", e))
            })?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                error!("Cohere API error ({}): {}", status, error_text);

                if status.as_u16() == HTTP_STATUS_RATE_LIMIT {
                    return Err(AppError::InternalServerError(
                        "Cohere API Rate limit exceeded".to_string(),
                    ));
                }

                return Err(AppError::InternalServerError(format!(
                    "Cohere API error: {}",
                    error_text
                )));
            }

            let result: CohereEmbedResponse = response.json().await.map_err(|e| {
                error!("Failed to parse Cohere response: {}", e);
                AppError::ExtractionError(format!("Failed to parse embedding response: {}", e))
            })?;

            all_embeddings.extend(result.embeddings);
        }

        Ok(all_embeddings)
    }

    fn get_dimension(&self) -> usize {
        self.dimension
    }
}
