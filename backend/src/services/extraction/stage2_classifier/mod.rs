pub mod heuristics;
pub mod noise_filter;

use crate::models::extraction_model::Slide;

pub fn classify_slide_elements(mut slide: Slide) -> Slide {
    for elem in &mut slide.elements {
        heuristics::classify_element(elem);
    }
    slide
}
