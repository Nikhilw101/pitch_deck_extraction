use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub db_name: String,
    pub llm_api_key: String,
    pub cohere_api_key: String,
    pub embedding_model: String,
    pub index_path: String,
    pub server_port: u16,
    pub ollama_model: String,
    pub ollama_base_url: String,
    pub frontend_origin: String,
}

/// Configuration error for missing required environment variables
#[derive(Debug)]
pub struct ConfigError {
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Load configuration from environment variables
///
/// # Required Variables
/// - `COHERE_API_KEY`: API key for Cohere embedding service (required for Phase 2)
///
/// # Optional Variables (with defaults)
/// - `DATABASE_URL`: MongoDB connection string (default: "mongodb://localhost:27017")
/// - `DB_NAME`: Database name (default: "pitch_deck_db")
/// - `LLM_API_KEY`: API key for LLM service (default: empty string)
/// - `EMBEDDING_MODEL`: Cohere embedding model name (default: "embed-english-v3.0")
/// - `INDEX_PATH`: Base path for HNSW vector index (default: "vector_index"; creates .hnsw.data and .meta.json)
/// - `PORT`: Server port (default: 3000)
/// - `OLLAMA_MODEL`: Local LLM model (default: "llama3.2:3b")
/// - `OLLAMA_BASE_URL`: Ollama API URL (default: "http://localhost:11434")
/// - `FRONTEND_ORIGIN`: Allowed frontend origin for CORS (default: "http://localhost:5173")
///
/// # Errors
/// Returns `ConfigError` if required environment variables are missing.
pub fn load() -> Result<Config, ConfigError> {
    dotenvy::dotenv().ok();

    // Validate required environment variables
    let cohere_api_key = env::var("COHERE_API_KEY")
        .map_err(|_| ConfigError {
            message: "COHERE_API_KEY environment variable is required but not set. Please set it in your .env file or environment.".to_string(),
        })?;

    if cohere_api_key.is_empty() {
        return Err(ConfigError {
            message: "COHERE_API_KEY environment variable is set but empty. Please provide a valid API key.".to_string(),
        });
    }

    Ok(Config {
        database_url: env::var("DATABASE_URL")
            .unwrap_or_else(|_| "mongodb://localhost:27017".to_string()),
        db_name: env::var("DB_NAME").unwrap_or_else(|_| "pitch_deck_db".to_string()),
        llm_api_key: env::var("LLM_API_KEY").unwrap_or_default(),
        cohere_api_key,
        embedding_model: env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "embed-english-v3.0".to_string()),
        index_path: env::var("INDEX_PATH").unwrap_or_else(|_| "vector_index".to_string()),
        server_port: env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("Warning: PORT invalid or unset, using 3000");
                3000
            }),
        ollama_model: env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".to_string()),
        ollama_base_url: env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        frontend_origin: env::var("FRONTEND_ORIGIN")
            .unwrap_or_else(|_| "http://localhost:5173".to_string()),
    })
}
