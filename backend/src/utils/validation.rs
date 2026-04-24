use crate::models::deck_model::*;

/// Validate table structure
pub fn validate_table(headers: &[String], rows: &[Vec<String>]) -> bool {
    if rows.is_empty() {
        return false;
    }

    let expected_cols = if !headers.is_empty() {
        headers.len()
    } else {
        rows[0].len()
    };

    rows.iter().all(|row| row.len() == expected_cols)
}

/// Validate chart data
pub fn validate_chart(chart: &crate::models::extraction_model::ChartData) -> bool {
    if chart.series.is_empty() {
        return false;
    }

    // All series should have same number of values
    let first_len = chart.series[0].values.len();
    chart.series.iter().all(|s| s.values.len() == first_len)
}

/// Validate position coordinates
pub fn validate_position(bbox: &BoundingBox) -> bool {
    (bbox.x1 - bbox.x0) > 0.0 && (bbox.y1 - bbox.y0) > 0.0
}

/// Check if slide has meaningful content
pub fn has_meaningful_content(slide: &Slide) -> bool {
    !slide.elements.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::extraction_model::{ChartData, Element, Series};

    #[test]
    fn test_validate_table() {
        assert!(validate_table(
            &[],
            &[vec!["A".to_string(), "B".to_string()]]
        ));
    }

    #[test]
    fn test_has_meaningful_content() {
        let slide = Slide {
            slide_number: 1,
            elements: vec![Element::Title {
                text: "Test".to_string(),
                bbox: None,
            }],
        };
        assert!(has_meaningful_content(&slide));
    }

    #[test]
    fn test_validate_table_empty_rows() {
        assert!(!validate_table(&[], &[]));
    }

    #[test]
    fn test_validate_chart() {
        let chart = ChartData {
            x_axis: None,
            series: vec![
                Series {
                    name: "Series1".to_string(),
                    values: vec![1.0, 2.0, 3.0],
                },
                Series {
                    name: "Series2".to_string(),
                    values: vec![4.0, 5.0, 6.0],
                },
            ],
        };
        assert!(validate_chart(&chart));
    }

    #[test]
    fn test_validate_chart_empty_series() {
        let chart = ChartData {
            x_axis: None,
            series: vec![],
        };
        assert!(!validate_chart(&chart));
    }

    #[test]
    fn test_validate_chart_mismatched_lengths() {
        let chart = ChartData {
            x_axis: None,
            series: vec![
                Series {
                    name: "Series1".to_string(),
                    values: vec![1.0, 2.0],
                },
                Series {
                    name: "Series2".to_string(),
                    values: vec![3.0, 4.0, 5.0],
                },
            ],
        };
        assert!(!validate_chart(&chart));
    }

    #[test]
    fn test_validate_position() {
        let bbox = BoundingBox {
            x0: 0.0,
            y0: 0.0,
            x1: 100.0,
            y1: 50.0,
        };
        assert!(validate_position(&bbox));
    }

    #[test]
    fn test_validate_position_invalid() {
        let bbox = BoundingBox {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 50.0,
        };
        assert!(!validate_position(&bbox));
    }

    #[test]
    fn test_has_meaningful_content_empty() {
        let slide = Slide {
            slide_number: 1,
            elements: vec![],
        };
        assert!(!has_meaningful_content(&slide));
    }
}
