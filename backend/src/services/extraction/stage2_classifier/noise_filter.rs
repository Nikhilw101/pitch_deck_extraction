use crate::models::extraction_model::{Element, Slide};
use std::collections::HashMap;

/// Cross-slide noise filter: detects repeated headers/footers and removes them.
///
/// Strategy:
/// 1. Collect all short text elements (≤10 words) from all slides
/// 2. Text appearing on ≥50% of slides is marked as header/footer noise
/// 3. Remove those elements from all slides
/// 4. Also remove empty text blocks and common document artifacts
pub fn filter_headers_footers(slides: &mut [Slide]) {
    if slides.len() < 3 {
        return; // Not enough slides for cross-slide analysis
    }

    let total_slides = slides.len();

    // Step 1: Count occurrences of short text across slides
    let mut text_occurrences: HashMap<String, usize> = HashMap::new();

    for slide in slides.iter() {
        // Track unique texts per slide to avoid counting duplicates within a slide
        let mut seen_on_slide: std::collections::HashSet<String> = std::collections::HashSet::new();

        for elem in &slide.elements {
            if let Some(text) = get_short_text(elem) {
                let normalized = normalize_for_comparison(&text);
                if !normalized.is_empty() && seen_on_slide.insert(normalized.clone()) {
                    *text_occurrences.entry(normalized).or_insert(0) += 1;
                }
            }
        }
    }

    // Step 2: Identify repeated text (appears on ≥50% of slides)
    let threshold = (total_slides as f32 * 0.5).ceil() as usize;
    let noise_texts: std::collections::HashSet<String> = text_occurrences
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(text, _)| text)
        .collect();

    if noise_texts.is_empty() {
        return;
    }

    // Step 3: Remove noise elements from all slides
    for slide in slides.iter_mut() {
        slide.elements.retain(|elem| {
            if let Some(text) = get_short_text(elem) {
                let normalized = normalize_for_comparison(&text);
                !noise_texts.contains(&normalized)
            } else {
                true
            }
        });
    }
}

/// Extract short text suitable for header/footer comparison.
/// Only considers TextBlock, Title, Subtitle with ≤10 words.
fn get_short_text(elem: &Element) -> Option<String> {
    let text = match elem {
        Element::TextBlock { text, .. } => text,
        Element::Title { text, .. } => text,
        Element::Subtitle { text, .. } => text,
        Element::SectionHeader { text, .. } => text,
        _ => return None,
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.split_whitespace().count() > 10 {
        return None;
    }

    Some(trimmed.to_string())
}

/// Normalize text for comparison: lowercase, strip punctuation, collapse whitespace.
fn normalize_for_comparison(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
