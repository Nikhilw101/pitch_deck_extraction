use serde::{Deserialize, Serialize};

pub use crate::models::extraction_model::{BoundingBox, Element, Slide};

/// Complete extracted deck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedDeck {
    pub deck_id: String,
    pub filename: String,
    pub file_type: FileType,
    pub total_slides: usize,
    pub slides: Vec<Slide>,
    pub metadata: DeckMetadata,
}

/// Deck-level metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckMetadata {
    pub extraction_timestamp: String,
    pub extraction_method: String,
    pub has_speaker_notes: bool,
    pub has_hidden_slides: bool,
    pub has_tables: bool,
    pub has_charts: bool,
}

/// File type enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Pdf,
    Pptx,
}

impl FileType {
    pub fn from_filename(filename: &str) -> Option<Self> {
        let ext = filename.split('.').next_back()?.to_lowercase();
        match ext.as_str() {
            "pdf" => Some(FileType::Pdf),
            "pptx" => Some(FileType::Pptx),
            _ => None,
        }
    }
}

/// Statistics and metadata about the indexing process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingMetadata {
    pub slides_indexed: usize,
    pub embedding_dimension: usize,
    pub status: String,
}

/// Unified response for deck processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResponse {
    pub deck_id: String,
    pub filename: String,
    pub file_type: FileType,
    pub total_slides: usize,
    pub metadata: DeckMetadata,
    pub indexing: IndexingMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouped_deck: Option<crate::models::section_model::GroupedDeck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<crate::models::structured_output::StructuredDeckOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobIdResponse {
    pub job_id: String,
    pub status: String,
}

