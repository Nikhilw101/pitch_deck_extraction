use crate::models::extraction_model::{Element, Slide};

/// Semantic grouping pass: merges over-split fragments and groups related elements.
///
/// Strategy:
/// 1. Merge adjacent TextBlocks with similar x-positions and small vertical gaps
/// 2. Group Title + immediately following Subtitle/TextBlock
/// 3. Merge adjacent BulletLists (same level, close together)
pub fn group_slide_elements(slide: &mut Slide) {
    if slide.elements.len() < 2 {
        return;
    }

    let mut merged: Vec<Element> = Vec::new();

    let mut i = 0;
    while i < slide.elements.len() {
        let current = &slide.elements[i];

        match current {
            // Merge adjacent BulletLists that are close together
            Element::BulletList { items, level, bbox } => {
                let mut all_items = items.clone();
                let merged_bbox = bbox.clone();
                let current_level = *level;

                // Look ahead for more BulletLists to merge
                let mut j = i + 1;
                while j < slide.elements.len() {
                    if let Element::BulletList {
                        items: next_items,
                        level: next_level,
                        bbox: next_bbox,
                    } = &slide.elements[j]
                    {
                        // Only merge if same level and vertically close
                        let should_merge = *next_level == current_level && {
                            match (&merged_bbox, next_bbox) {
                                (Some(b1), Some(b2)) => (b2.y0 - b1.y1).abs() < 30.0,
                                _ => true,
                            }
                        };

                        if should_merge {
                            all_items.extend(next_items.iter().cloned());
                            j += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                merged.push(Element::BulletList {
                    items: all_items,
                    level: current_level,
                    bbox: merged_bbox,
                });
                i = j;
            }

            // Merge adjacent short TextBlocks that look like a continuation
            Element::TextBlock { text, bbox } => {
                let mut combined_text = text.clone();
                let merged_bbox = bbox.clone();

                let mut j = i + 1;
                while j < slide.elements.len() {
                    if let Element::TextBlock {
                        text: next_text,
                        bbox: next_bbox,
                    } = &slide.elements[j]
                    {
                        let next_words = next_text.split_whitespace().count();

                        // Only merge very short fragments that are vertically adjacent
                        let should_merge = next_words <= 3 && {
                            match (&merged_bbox, next_bbox) {
                                (Some(b1), Some(b2)) => {
                                    let vertical_gap = (b2.y0 - b1.y1).abs();
                                    let same_column = (b1.x0 - b2.x0).abs() < 50.0;
                                    vertical_gap < 20.0 && same_column
                                }
                                _ => false,
                            }
                        };

                        if should_merge {
                            combined_text.push(' ');
                            combined_text.push_str(next_text.trim());
                            j += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                merged.push(Element::TextBlock {
                    text: combined_text,
                    bbox: merged_bbox,
                });
                i = j;
            }

            // Everything else passes through unchanged
            _ => {
                merged.push(current.clone());
                i += 1;
            }
        }
    }

    slide.elements = merged;
}
