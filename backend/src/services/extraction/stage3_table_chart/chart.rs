use crate::models::extraction_model::{ChartData, ChartType, Element, Series};

pub fn detect_chart(elem: &mut Element) {
    if let Element::Image {
        path,
        bbox,
        ocr_text: _,
    } = elem
    {
        // Simple heuristic: If the filename contains "chart" or "graph"
        // Not very robust but requested as a rule-based starting point.
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.contains("chart") || path_str.contains("graph") || path_str.contains("plot") {
            // Stub extraction logic
            // Real implementation would use opencv to detect rects/contours.

            *elem = Element::Chart {
                chart_type: ChartType::Unknown,
                title: None,
                data: ChartData {
                    x_axis: Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
                    series: vec![Series {
                        name: "Placeholder".to_string(),
                        values: vec![10.0, 20.0, 30.0],
                    }],
                },
                bbox: bbox.clone(),
            };
        }
    }
}
