use crate::errors::app_error::AppError;
use crate::models::vector_model::{EmbeddingRecord, SearchResult};
use hnsw_rs::prelude::*;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Vector store for semantic search using HNSW (Hierarchical Navigable Small World) index.
///
/// Provides fast approximate nearest neighbor search for embedding vectors.
/// Supports persistence to disk and loading existing indices.
pub struct VectorStore {
    hnsw: Arc<RwLock<Hnsw<'static, f32, DistL2>>>,
    metadata: Arc<RwLock<Vec<EmbeddingRecord>>>,
    index_path: String,
}

impl VectorStore {
    /// Create a new vector store, loading existing index if available.
    ///
    /// # Arguments
    /// * `_dimension` - Dimension of embedding vectors (must match stored vectors)
    ///   Note: Currently not validated, but reserved for future dimension checking
    /// * `index_path` - Base path for index files (`.hnsw.data` and `.meta.json` will be appended)
    ///
    /// # Returns
    /// * `Ok(VectorStore)` - Successfully created or loaded vector store
    /// * `Err(AppError)` - Error if index loading fails
    ///
    /// # Behavior
    /// - If index files exist, loads them from disk
    /// - Otherwise, creates a new empty index
    /// - Index is persisted automatically after each `add_vectors` call
    pub fn new(_dimension: usize, index_path: String) -> Result<Self, AppError> {
        let metadata_path = format!("{}.meta.json", index_path);
        let hnsw_data_path = format!("{}.hnsw.data", index_path);

        let (hnsw, metadata) = if Path::new(&hnsw_data_path).exists()
            && Path::new(&metadata_path).exists()
        {
            info!("Loading existing HNSW index from {}", index_path);

            let path = Path::new(&index_path);
            let directory = path.parent().unwrap_or(Path::new("."));
            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("index");

            // NOTE: Intentional memory leak for HNSW reloader
            // The `hnsw_rs` library requires a 'static lifetime for the HnswIo reloader
            // when loading an existing index. Since VectorStore lives for the application
            // lifetime, leaking this Box is acceptable and necessary. The memory will be
            // reclaimed when the process exits. This is a known pattern for long-lived
            // resources in Rust when 'static lifetime is required by external libraries.
            // Alternative approaches (like using unsafe or different library) would be
            // more complex and potentially unsafe.
            let reloader = Box::leak(Box::new(HnswIo::new(directory, filename)));
            let hnsw = reloader.load_hnsw::<f32, DistL2>().map_err(|e| {
                error!("Failed to load HNSW index: {:?}", e);
                AppError::InternalServerError("Failed to load HNSW index".to_string())
            })?;

            let meta_json = std::fs::read_to_string(&metadata_path).map_err(|e| {
                error!("Failed to read HNSW metadata: {}", e);
                AppError::InternalServerError("Failed to load HNSW metadata".to_string())
            })?;

            let metadata: Vec<EmbeddingRecord> = serde_json::from_str(&meta_json).map_err(|e| {
                error!("Failed to parse HNSW metadata: {}", e);
                AppError::InternalServerError("Failed to parse HNSW metadata".to_string())
            })?;

            (hnsw, metadata)
        } else {
            info!("Creating new HNSW index at {}", index_path);
            let hnsw = Hnsw::new(16, 1000000, 16, 200, DistL2 {});
            (hnsw, Vec::new())
        };

        Ok(Self {
            hnsw: Arc::new(RwLock::new(hnsw)),
            metadata: Arc::new(RwLock::new(metadata)),
            index_path,
        })
    }

    /// Add embedding vectors to the index with associated metadata.
    ///
    /// # Arguments
    /// * `vectors` - Embedding vectors to add (must match store dimension)
    /// * `records` - Metadata records corresponding to each vector
    ///
    /// # Returns
    /// * `Ok(())` - Successfully added and persisted
    /// * `Err(AppError)` - Error if vectors/records mismatch or persistence fails
    ///
    /// # Note
    /// The index is automatically persisted to disk after adding vectors.
    /// This operation takes a write lock and may block concurrent searches.
    pub async fn add_vectors(
        &self,
        vectors: Vec<Vec<f32>>,
        records: Vec<EmbeddingRecord>,
    ) -> Result<(), AppError> {
        let hnsw = self.hnsw.write().await;
        let mut metadata = self.metadata.write().await;

        if vectors.len() != records.len() {
            return Err(AppError::InternalServerError(
                "Vector and metadata size mismatch".to_string(),
            ));
        }

        let inserts: Vec<(&Vec<f32>, usize)> = vectors
            .iter()
            .enumerate()
            .map(|(i, vec)| (vec, metadata.len() + i))
            .collect();

        hnsw.parallel_insert(&inserts);

        metadata.extend(records);

        info!("Added vectors to HNSW index (Total: {})", metadata.len());

        self.save_internal(&hnsw, &metadata).await?;

        Ok(())
    }

    /// Search for the k nearest neighbors to a query vector.
    ///
    /// Uses approximate nearest neighbor search with HNSW algorithm for fast retrieval.
    ///
    /// # Arguments
    /// * `query_vector` - Query embedding vector (must match store dimension)
    /// * `k` - Number of nearest neighbors to return
    ///
    /// # Returns
    /// * `Ok(Vec<SearchResult>)` - Search results sorted by distance (lower = more similar)
    /// * `Err(AppError)` - Error if search fails
    ///
    /// # Note
    /// Distance scores use L2 (Euclidean) distance. Lower scores indicate higher similarity.
    pub async fn search(
        &self,
        query_vector: &[f32],
        k: usize,
    ) -> Result<Vec<SearchResult>, AppError> {
        let hnsw = self.hnsw.read().await;
        let metadata = self.metadata.read().await;

        // hnsw_rs 0.3.3 search expects &Vec<f32> when T=f32 for some reason (based on compiler feedback)
        let query_vec = query_vector.to_vec();
        let results = hnsw.search(&query_vec, k, 100);

        let mut search_results = Vec::new();
        for neighbor in results {
            let idx = neighbor.d_id;
            if let Some(record) = metadata.get(idx) {
                search_results.push(SearchResult {
                    deck_id: record.deck_id.clone(),
                    slide_number: record.slide_number,
                    score: neighbor.distance,
                    text_snippet: Some(record.text.clone()),
                });
            }
        }

        Ok(search_results)
    }

    /// Search for the k nearest neighbors within a specific deck.
    ///
    /// Similar to `search`, but filters results to only include slides from the specified deck.
    ///
    /// # Arguments
    /// * `query_vector` - Query embedding vector (must match store dimension)
    /// * `deck_id` - Deck ID to filter results by
    /// * `k` - Number of nearest neighbors to return
    ///
    /// # Returns
    /// * `Ok(Vec<SearchResult>)` - Search results filtered to the specified deck
    /// * `Err(AppError)` - Error if search fails
    pub async fn search_within_deck(
        &self,
        query_vector: &[f32],
        deck_id: &str,
        k: usize,
    ) -> Result<Vec<SearchResult>, AppError> {
        // Search more results than needed, then filter by deck_id
        let all_results = self.search(query_vector, k * 2).await?;

        // Filter to only this deck and limit to k results
        let filtered: Vec<SearchResult> = all_results
            .into_iter()
            .filter(|r| r.deck_id == deck_id)
            .take(k)
            .collect();

        Ok(filtered)
    }

    pub async fn save(&self) -> Result<(), AppError> {
        let hnsw = self.hnsw.read().await;
        let metadata = self.metadata.read().await;
        self.save_internal(&hnsw, &metadata).await
    }

    async fn save_internal(
        &self,
        hnsw: &Hnsw<'_, f32, DistL2>,
        metadata: &Vec<EmbeddingRecord>,
    ) -> Result<(), AppError> {
        info!("Persisting HNSW index and metadata to disk...");

        let path = Path::new(&self.index_path);
        let directory = path.parent().unwrap_or(Path::new("."));
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("index");

        hnsw.file_dump(directory, filename).map_err(|e| {
            error!("Failed to save HNSW index: {:?}", e);
            AppError::InternalServerError("Failed to save HNSW index".to_string())
        })?;

        let metadata_path = format!("{}.meta.json", self.index_path);
        let meta_json = serde_json::to_string(metadata).map_err(|e| {
            error!("Failed to serialize HNSW metadata: {}", e);
            AppError::InternalServerError("Failed to serialize metadata".to_string())
        })?;

        std::fs::write(&metadata_path, meta_json).map_err(|e| {
            error!("Failed to write HNSW metadata: {}", e);
            AppError::InternalServerError("Failed to write metadata".to_string())
        })?;

        info!("HNSW persistence successful");
        Ok(())
    }
}
