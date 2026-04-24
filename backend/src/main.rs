use pitch_deck_service::config;
use pitch_deck_service::services::embedding_service::EmbeddingService;
use pitch_deck_service::services::llm_service::LlmService;
use pitch_deck_service::utils::logger;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    logger::init();

    // Load configuration
    let config = config::load().map_err(|e| {
        eprintln!("Configuration error: {}", e);
        eprintln!("\nPlease ensure all required environment variables are set.");
        eprintln!("See .env.example or documentation for required variables.");
        anyhow::anyhow!("Configuration failed: {}", e)
    })?;

    info!("Starting Pitch Deck Extractor Service...");

    // Connect to MongoDB
    let client = pitch_deck_service::db::connection::init_db(&config.database_url).await?;
    let _db = client.database(&config.db_name);
    info!("Connected to MongoDB: {}", config.db_name);

    // Initialize Phase 2 Services
    let embedding_service = std::sync::Arc::new(
        pitch_deck_service::services::embedding_service::CohereClient::new(
            config.cohere_api_key.clone(),
            config.embedding_model.clone(),
        )?,
    );

    let vector_store = std::sync::Arc::new(
        pitch_deck_service::services::vector_store_service::VectorStore::new(
            embedding_service.get_dimension(),
            config.index_path.clone(),
        )?,
    );

    let pipeline_service = std::sync::Arc::new(
        pitch_deck_service::services::pipeline_service::PipelineService::new(
            embedding_service.clone(),
            vector_store.clone(),
        ),
    );

    // Initialize Phase 4 LLM Service (Ollama)
    let llm_service = std::sync::Arc::new(
        pitch_deck_service::services::llm_service::OllamaClient::new(
            Some(config.ollama_model.clone()),
            Some(config.ollama_base_url.clone()),
        )?,
    );
    info!(
        "Initialized Ollama LLM service (model: {}, url: {})",
        config.ollama_model, config.ollama_base_url
    );

    // Initialize Job Service for background processing
    let job_service = std::sync::Arc::new(pitch_deck_service::services::job_service::JobService::new());


    // Build our application with a route
    let allow_origin = config
        .frontend_origin
        .parse()
        .map(AllowOrigin::exact)
        .unwrap_or_else(|_| {
            eprintln!(
                "Warning: FRONTEND_ORIGIN '{}' is invalid, defaulting to http://localhost:5173",
                config.frontend_origin
            );
            AllowOrigin::exact(
                "http://localhost:5173"
                    .parse()
                    .expect("hardcoded default origin must be valid"),
            )
        });
    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    let app = pitch_deck_service::routes::create_router()
        .layer(cors)
        .layer(axum::extract::Extension(
            embedding_service as Arc<dyn EmbeddingService>,
        ))
        .layer(axum::extract::Extension(vector_store))
        .layer(axum::extract::Extension(pipeline_service))
        .layer(axum::extract::Extension(
            llm_service.clone() as Arc<dyn LlmService>
        ))
        .layer(axum::extract::Extension(job_service));

    // Run it (use config as single source of truth for port)
    let port = config.server_port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
