use crate::models::extraction_model::{BoundingBox, Element, Slide};
use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{info, warn};
use zip::ZipArchive;

/// Parse a PPTX file by traversing the ZIP archive and extracting slide XML.
///
/// This implementation:
/// 1. Opens the PPTX as a ZIP archive
/// 2. Discovers and sorts slide XML files (ppt/slides/slideN.xml)
/// 3. Parses each slide's XML to extract text shapes, tables, and images
/// 4. Extracts speaker notes from ppt/notesSlides/
/// 5. Preserves shape positions as bounding boxes
pub fn parse_pptx(path: &Path) -> Result<Vec<Slide>> {
    info!("Parsing PPTX file: {:?}", path);

    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("Failed to open PPTX as zip")?;

    // Collect all file names first to avoid borrow conflicts
    let file_names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    // Find and sort slide files numerically
    let mut slide_files: Vec<(usize, String)> = file_names
        .iter()
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .filter_map(|name| {
            let num_str = name
                .trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml");
            num_str.parse::<usize>().ok().map(|n| (n, name.clone()))
        })
        .collect();
    slide_files.sort_by_key(|(n, _)| *n);

    // Extract speaker notes mapping: slide_num -> notes_text
    let notes_map = extract_all_notes(&mut archive, &file_names);

    let mut slides = Vec::new();

    for (slide_num, slide_path) in &slide_files {
        let xml_content = match read_zip_entry(&mut archive, slide_path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read slide {}: {}", slide_path, e);
                continue;
            }
        };

        let mut elements = parse_slide_xml(&xml_content);

        // If no elements found, add a placeholder
        if elements.is_empty() {
            elements.push(Element::TextBlock {
                text: String::new(),
                bbox: None,
            });
        }

        slides.push(Slide {
            slide_number: *slide_num as u32,
            elements,
        });
    }

    // Attach speaker notes to the corresponding slide elements
    for (slide_num, notes) in &notes_map {
        if !notes.trim().is_empty() {
            if let Some(slide) = slides
                .iter_mut()
                .find(|s| s.slide_number == *slide_num as u32)
            {
                slide.elements.push(Element::TextBlock {
                    text: format!("Speaker Notes: {}", notes),
                    bbox: None,
                });
            }
        }
    }

    if slides.is_empty() {
        anyhow::bail!("No slides could be extracted from PPTX file");
    }

    info!("Successfully extracted {} slides from PPTX", slides.len());
    Ok(slides)
}

/// Read a specific entry from a ZIP archive as a String.
fn read_zip_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
    let mut entry = archive.by_name(name)?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    Ok(content)
}

/// Extract all speaker notes from the archive.
fn extract_all_notes(
    archive: &mut ZipArchive<File>,
    file_names: &[String],
) -> std::collections::HashMap<usize, String> {
    let mut notes_map = std::collections::HashMap::new();

    for name in file_names {
        if name.starts_with("ppt/notesSlides/notesSlide") && name.ends_with(".xml") {
            let num_str = name
                .trim_start_matches("ppt/notesSlides/notesSlide")
                .trim_end_matches(".xml");
            if let Ok(slide_num) = num_str.parse::<usize>() {
                if let Ok(xml) = read_zip_entry(archive, name) {
                    let text = extract_text_from_xml(&xml);
                    if !text.is_empty() {
                        notes_map.insert(slide_num, text);
                    }
                }
            }
        }
    }

    notes_map
}

/// Extract all `<a:t>` text from an XML string (used for notes).
fn extract_text_from_xml(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut texts = Vec::new();
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"a:t" => in_text = true,
            Ok(Event::Text(e)) if in_text => {
                texts.push(e.unescape().unwrap_or_default().to_string());
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"a:t" => in_text = false,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    texts.join(" ")
}

/// Parse a single slide's XML to extract structured elements with positions.
///
/// Handles:
/// - Text shapes (`<p:sp>` with `<a:t>` nodes)
/// - Shape positions (`<a:off>` and `<a:ext>` for bounding boxes)
/// - Tables (`<a:tbl>` with rows and cells)
/// - Bullet lists (detected by `<a:buChar>` or `<a:buAutoNum>`)
fn parse_slide_xml(xml: &str) -> Vec<Element> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut elements = Vec::new();

    // Track shape-level state
    let mut in_shape = false;
    let mut in_text_body = false;
    let mut in_paragraph = false;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_nv_sp_pr = false;

    // Current shape data
    let mut current_texts: Vec<String> = Vec::new();
    let mut current_paragraph_texts: Vec<String> = Vec::new();
    let mut has_bullet = false;
    let mut shape_bbox: Option<BoundingBox> = None;
    let mut placeholder_type: Option<String> = None;

    // Table data
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row_cells: Vec<String> = Vec::new();
    let mut in_table_cell = false;
    let mut table_cell_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"p:sp" => {
                        in_shape = true;
                        current_texts.clear();
                        current_paragraph_texts.clear();
                        has_bullet = false;
                        shape_bbox = None;
                        placeholder_type = None;
                    }
                    b"p:nvSpPr" => {
                        in_nv_sp_pr = true;
                    }
                    b"p:pic" => {
                        // Image/Picture support
                        in_shape = true;
                        shape_bbox = None;
                    }
                    b"p:txBody" => {
                        in_text_body = true;
                    }
                    b"a:p" if in_text_body => {
                        in_paragraph = true;
                        current_paragraph_texts.clear();
                    }
                    b"a:t" => {
                        in_text = true;
                    }
                    b"a:buChar" | b"a:buAutoNum" => {
                        has_bullet = true;
                    }
                    b"a:off" if in_shape => {
                        // Extract position: <a:off x="..." y="..."/>
                        let (x, y) = extract_xy_attrs(e);
                        if let (Some(x), Some(y)) = (x, y) {
                            let existing = shape_bbox.take();
                            shape_bbox = Some(BoundingBox::new(
                                emu_to_points(x),
                                emu_to_points(y),
                                existing.as_ref().map(|b| b.x1).unwrap_or(emu_to_points(x)),
                                existing.as_ref().map(|b| b.y1).unwrap_or(emu_to_points(y)),
                            ));
                        }
                    }
                    b"a:ext" if in_shape => {
                        // Extract extent: <a:ext cx="..." cy="..."/>
                        let (cx, cy) = extract_cxcy_attrs(e);
                        if let (Some(cx), Some(cy)) = (cx, cy) {
                            if let Some(ref mut bb) = shape_bbox {
                                bb.x1 = bb.x0 + emu_to_points(cx);
                                bb.y1 = bb.y0 + emu_to_points(cy);
                            }
                        }
                    }
                    b"a:tbl" => {
                        in_table = true;
                        table_rows.clear();
                        shape_bbox = None;
                    }
                    b"a:tr" if in_table => {
                        current_row_cells.clear();
                    }
                    b"a:tc" if in_table => {
                        in_table_cell = true;
                        table_cell_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) if in_text => {
                let text = e.unescape().unwrap_or_default().to_string();
                if in_table_cell {
                    table_cell_text.push_str(&text);
                } else if in_paragraph {
                    current_paragraph_texts.push(text);
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"a:t" => {
                        in_text = false;
                    }
                    b"a:p" if in_text_body => {
                        in_paragraph = false;
                        let para_text = current_paragraph_texts.join("");
                        if !para_text.trim().is_empty() {
                            current_texts.push(para_text);
                        }
                    }
                    b"p:txBody" => {
                        in_text_body = false;
                    }
                    b"p:nvSpPr" => {
                        in_nv_sp_pr = false;
                    }
                    b"p:sp" => {
                        // Flush shape content using placeholder type for smart classification
                        if !current_texts.is_empty() {
                            let ph_type = placeholder_type.as_deref().unwrap_or("");

                            match ph_type {
                                "title" | "ctrTitle" => {
                                    // Title placeholder → emit as Title
                                    let combined = current_texts.join("\n");
                                    elements.push(Element::Title {
                                        text: combined,
                                        bbox: shape_bbox.clone(),
                                    });
                                }
                                "subTitle" => {
                                    // Subtitle placeholder → emit as Subtitle
                                    let combined = current_texts.join("\n");
                                    elements.push(Element::Subtitle {
                                        text: combined,
                                        bbox: shape_bbox.clone(),
                                    });
                                }
                                _ if has_bullet && !current_texts.is_empty() => {
                                    // Bullet list shape
                                    elements.push(Element::BulletList {
                                        items: current_texts.clone(),
                                        level: None,
                                        bbox: shape_bbox.clone(),
                                    });
                                }
                                _ if current_texts.len() == 1 => {
                                    // Single paragraph → one TextBlock
                                    elements.push(Element::TextBlock {
                                        text: current_texts[0].clone(),
                                        bbox: shape_bbox.clone(),
                                    });
                                }
                                _ => {
                                    // Multiple paragraphs without bullets →
                                    // Emit each as a separate TextBlock for better classification
                                    for para_text in &current_texts {
                                        elements.push(Element::TextBlock {
                                            text: para_text.clone(),
                                            bbox: shape_bbox.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        in_shape = false;
                        current_texts.clear();
                        shape_bbox = None;
                        has_bullet = false;
                        placeholder_type = None;
                    }
                    b"p:pic" => {
                        elements.push(Element::Image {
                            path: std::path::PathBuf::from("embedded_image"),
                            bbox: shape_bbox.clone(),
                            ocr_text: None,
                        });
                        in_shape = false;
                        shape_bbox = None;
                    }
                    b"a:tc" if in_table => {
                        in_table_cell = false;
                        current_row_cells.push(table_cell_text.trim().to_string());
                    }
                    b"a:tr" if in_table => {
                        if !current_row_cells.is_empty() {
                            table_rows.push(current_row_cells.clone());
                        }
                    }
                    b"a:tbl" => {
                        // Flush table
                        if !table_rows.is_empty() {
                            let headers = if table_rows.len() > 1 {
                                table_rows.remove(0)
                            } else {
                                vec![]
                            };
                            elements.push(Element::Table {
                                headers,
                                rows: table_rows.clone(),
                                bbox: shape_bbox.clone(), // Use shape_bbox if available for table
                            });
                            table_rows.clear();
                        }
                        in_table = false;
                        shape_bbox = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                match e.name().as_ref() {
                    b"a:buChar" | b"a:buAutoNum" => {
                        has_bullet = true;
                    }
                    b"p:ph" if in_nv_sp_pr => {
                        // Extract placeholder type: <p:ph type="title"/>
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                placeholder_type =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                        // If no type attribute, it's a body placeholder
                        if placeholder_type.is_none() {
                            placeholder_type = Some("body".to_string());
                        }
                    }
                    b"a:off" if in_shape => {
                        let (x, y) = extract_xy_attrs(e);
                        if let (Some(x), Some(y)) = (x, y) {
                            let existing = shape_bbox.take();
                            shape_bbox = Some(BoundingBox::new(
                                emu_to_points(x),
                                emu_to_points(y),
                                existing.as_ref().map(|b| b.x1).unwrap_or(emu_to_points(x)),
                                existing.as_ref().map(|b| b.y1).unwrap_or(emu_to_points(y)),
                            ));
                        }
                    }
                    b"a:ext" if in_shape => {
                        let (cx, cy) = extract_cxcy_attrs(e);
                        if let (Some(cx), Some(cy)) = (cx, cy) {
                            if let Some(ref mut bb) = shape_bbox {
                                bb.x1 = bb.x0 + emu_to_points(cx);
                                bb.y1 = bb.y0 + emu_to_points(cy);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("XML parsing error in slide: {}", e);
                break;
            }
            _ => {}
        }
    }

    elements
}

/// Extract x, y attributes from an element (for `<a:off x="..." y="..."/>`)
fn extract_xy_attrs(e: &quick_xml::events::BytesStart) -> (Option<i64>, Option<i64>) {
    let mut x = None;
    let mut y = None;
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"x" => x = String::from_utf8_lossy(&attr.value).parse().ok(),
            b"y" => y = String::from_utf8_lossy(&attr.value).parse().ok(),
            _ => {}
        }
    }
    (x, y)
}

/// Extract cx, cy attributes from an element (for `<a:ext cx="..." cy="..."/>`)
fn extract_cxcy_attrs(e: &quick_xml::events::BytesStart) -> (Option<i64>, Option<i64>) {
    let mut cx = None;
    let mut cy = None;
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"cx" => cx = String::from_utf8_lossy(&attr.value).parse().ok(),
            b"cy" => cy = String::from_utf8_lossy(&attr.value).parse().ok(),
            _ => {}
        }
    }
    (cx, cy)
}

/// Convert EMU (English Metric Units) to points.
/// 1 inch = 914400 EMU = 72 points
fn emu_to_points(emu: i64) -> f32 {
    (emu as f64 / 914400.0 * 72.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emu_to_points() {
        // 1 inch = 914400 EMU = 72 points
        let pts = emu_to_points(914400);
        assert!((pts - 72.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_text_from_xml() {
        let xml = r#"<root><a:t>Hello</a:t> <a:t>World</a:t></root>"#;
        let text = extract_text_from_xml(xml);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_parse_slide_xml_basic() {
        let xml = r#"
        <p:sld>
            <p:cSld>
                <p:spTree>
                    <p:sp>
                        <p:txBody>
                            <a:p><a:r><a:t>Title Text</a:t></a:r></a:p>
                            <a:p><a:r><a:t>Body text here</a:t></a:r></a:p>
                        </p:txBody>
                    </p:sp>
                </p:spTree>
            </p:cSld>
        </p:sld>
        "#;
        let elements = parse_slide_xml(xml);
        assert!(!elements.is_empty(), "Should extract at least one element");
    }

    #[test]
    fn test_parse_slide_xml_with_bullets() {
        let xml = r#"
        <p:sld>
            <p:cSld>
                <p:spTree>
                    <p:sp>
                        <p:txBody>
                            <a:p><a:pPr><a:buChar char="•"/></a:pPr><a:r><a:t>Item 1</a:t></a:r></a:p>
                            <a:p><a:pPr><a:buChar char="•"/></a:pPr><a:r><a:t>Item 2</a:t></a:r></a:p>
                            <a:p><a:pPr><a:buChar char="•"/></a:pPr><a:r><a:t>Item 3</a:t></a:r></a:p>
                        </p:txBody>
                    </p:sp>
                </p:spTree>
            </p:cSld>
        </p:sld>
        "#;
        let elements = parse_slide_xml(xml);
        assert!(!elements.is_empty());
        // Should detect bullet list
        let has_bullets = elements
            .iter()
            .any(|e| matches!(e, Element::BulletList { .. }));
        assert!(has_bullets, "Should detect bullet list from <a:buChar>");
    }
}
