pub mod chart;
pub mod table;

use crate::models::extraction_model::Slide;

pub fn extract_tables_and_charts(mut slide: Slide) -> Slide {
    for elem in &mut slide.elements {
        table::refine_table(elem);
        chart::detect_chart(elem);
    }
    slide
}
