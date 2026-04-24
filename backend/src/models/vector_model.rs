use serde::{Deserialize, Serialize};

/// Metadata to map a vector index to a specific slide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    pub deck_id: String,
    pub slide_number: usize,
    pub text: String, // Added for search snippets
    pub text_hash: String,
}

/// Search result from vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub deck_id: String,
    pub slide_number: usize,
    pub score: f32,
    pub text_snippet: Option<String>,
}

/// Request for similarity search
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    5
}

/// Standardized search response
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}
