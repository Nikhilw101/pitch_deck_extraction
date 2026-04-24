use crate::errors::app_error::AppError;
use crate::models::deck_model::{DeckMetadata, ExtractedDeck};
use crate::services::extraction::process_pitch_deck;
use std::path::Path;
use tracing::info;

/// Extract content from a PDF or PPTX file using the unified 4-stage layout-aware pipeline.
pub async fn extract_document(
    file_path: &Path,
    filename: &str,
    file_type: &str,
) -> Result<ExtractedDeck, AppError> {
    info!(
        "Starting 4-stage extraction for {}: {}",
        file_type.to_uppercase(),
        filename
    );

    // Call the unified pipeline (Phase 1)
    let slides = process_pitch_deck(file_path, file_type)
        .await
        .map_err(|e| {
            AppError::ExtractionError(format!("Failed to process document {}: {}", filename, e))
        })?;

    Ok(ExtractedDeck {
        deck_id: uuid::Uuid::new_v4().to_string(),
        filename: filename.to_string(),
        file_type: match file_type {
            "pdf" => crate::models::deck_model::FileType::Pdf,
            "pptx" => crate::models::deck_model::FileType::Pptx,
            _ => crate::models::deck_model::FileType::Pdf,
        },
        total_slides: slides.len(),
        metadata: DeckMetadata {
            extraction_timestamp: chrono::Utc::now().to_rfc3339(),
            extraction_method: "unified_4_stage_pipeline".to_string(),
            has_speaker_notes: slides.iter().any(|s| {
                s.elements.iter().any(|e| match e {
                    crate::models::extraction_model::Element::TextBlock { text, .. } => {
                        text.starts_with("Speaker Notes:")
                    }
                    _ => false,
                })
            }),
            has_hidden_slides: false,
            has_tables: slides.iter().any(|s| {
                s.elements
                    .iter()
                    .any(|e| matches!(e, crate::models::extraction_model::Element::Table { .. }))
            }),
            has_charts: slides.iter().any(|s| {
                s.elements
                    .iter()
                    .any(|e| matches!(e, crate::models::extraction_model::Element::Chart { .. }))
            }),
        },
        slides,
    })
}

pub async fn extract_pdf(file_path: &Path, filename: &str) -> Result<ExtractedDeck, AppError> {
    extract_document(file_path, filename, "pdf").await
}

pub async fn extract_pptx(file_path: &Path, filename: &str) -> Result<ExtractedDeck, AppError> {
    extract_document(file_path, filename, "pptx").await
}
