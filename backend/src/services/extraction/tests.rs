use crate::models::extraction_model::{BoundingBox, Element, Slide};
use crate::services::extraction::{stage2_classifier, stage3_table_chart};

#[test]
fn test_classification_heuristics() {
    let mut slide = Slide {
        slide_number: 1,
        elements: vec![
            Element::TextBlock {
                text: "HELLO WORLD TITLE".to_string(),
                bbox: Some(BoundingBox::new(0.0, 50.0, 100.0, 70.0)),
            },
            Element::TextBlock {
                text: "• First bullet point\n• Second bullet point\n• Third bullet point".to_string(),
                bbox: None,
            },
            Element::TextBlock {
                text: "A longer text block that shouldn't be matched as a title because it is too long to be a title.".to_string(),
                bbox: None,
            },
        ],
    };

    slide = stage2_classifier::classify_slide_elements(slide);

    assert!(matches!(slide.elements[0], Element::Title { .. }));
    assert!(matches!(slide.elements[1], Element::BulletList { .. }));
    assert!(matches!(slide.elements[2], Element::TextBlock { .. }));
}

#[test]
fn test_table_refinement() {
    let mut slide = Slide {
        slide_number: 1,
        elements: vec![Element::Table {
            headers: vec![],
            rows: vec![
                vec!["Header A".to_string(), "Header B".to_string()],
                vec!["10".to_string(), "20".to_string()],
            ],
            bbox: None,
        }],
    };

    slide = stage3_table_chart::extract_tables_and_charts(slide);

    if let Element::Table { headers, rows, .. } = &slide.elements[0] {
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], "Header A");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "10");
    } else {
        panic!("Element was not a table");
    }
}
