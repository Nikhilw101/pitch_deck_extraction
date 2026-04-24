pub mod stage1_parser;
pub mod stage2_classifier;
pub mod stage3_table_chart;
pub mod stage4_grouping;

use crate::models::extraction_model::{Element, Slide};
use std::path::Path;

pub async fn process_pitch_deck(path: &Path, extension: &str) -> anyhow::Result<Vec<Slide>> {
    // Stage 1: Layout-Aware Parsing (PDF/PPTX → raw elements)
    let mut slides = stage1_parser::parse_document(path, extension).await?;

    // Stage 2a: Per-element classification (TextBlock → Title/Subtitle/BulletList etc.)
    for slide in &mut slides {
        let classified = stage2_classifier::classify_slide_elements(slide.clone());
        *slide = classified;
    }

    // Stage 2b: Cross-slide noise filtering (removes repeated headers/footers)
    stage2_classifier::noise_filter::filter_headers_footers(&mut slides);

    // Stage 3: Table & chart refinement
    for slide in &mut slides {
        *slide = stage3_table_chart::extract_tables_and_charts(slide.clone());
    }

    // Stage 4: Semantic grouping (merge over-split fragments, group BulletLists)
    for slide in &mut slides {
        stage4_grouping::group_slide_elements(slide);
    }

    // Final cleanup: remove empty text blocks
    for slide in &mut slides {
        slide.elements.retain(|e| match e {
            Element::TextBlock { text, .. } => !text.trim().is_empty(),
            _ => true,
        });
    }
    slides.retain(|s| !s.elements.is_empty());

    Ok(slides)
}

#[cfg(test)]
mod tests;
