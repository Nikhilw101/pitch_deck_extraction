use crate::models::extraction_model::{BoundingBox, Element, Slide};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct PySlide {
    slide_number: u32,
    elements: Vec<PyElement>,
}

#[derive(Debug, Deserialize)]
struct PyElement {
    #[serde(rename = "type")]
    element_type: String,
    text: Option<String>,
    /// items for bullet lists (may also be used as generic items)
    items: Option<Vec<String>>,
    level: Option<u8>,
    bbox: Option<(f32, f32, f32, f32)>,
    headers: Option<Vec<String>>,
    rows: Option<Vec<Vec<String>>>,
}

/// Structured output from the Python extractor script.
#[derive(Debug, Deserialize)]
struct PyOutput {
    /// Status string from the script ("ok" | "error") — kept for logging only.
    #[allow(dead_code)]
    status: Option<String>,
    /// Which backend was actually used (pdfplumber / pymupdf / pypdf).
    backend: Option<String>,
    slides: Option<Vec<PySlide>>,
    error: Option<String>,
}

pub async fn parse_pdf(path: &Path) -> Result<Vec<Slide>> {
    info!("Parsing PDF file via python script: {:?}", path);

    let script_path = Path::new("scripts").join("pdf_extractor.py");
    let output = Command::new("python")
        .arg(&script_path)
        .arg(path)
        .output()
        .await
        .context("Failed to run python script for PDF extraction")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        warn!("Python script stderr: {}", stderr);
        anyhow::bail!("Python script failed: {}", stderr);
    }

    if !stderr.is_empty() {
        warn!("Python script warnings: {}", stderr);
    }

    let result: PyOutput = serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse JSON output from python script: {}", stdout))?;

    if let Some(err) = result.error {
        anyhow::bail!("Extraction error: {}", err);
    }

    let backend = result.backend.unwrap_or_else(|| "unknown".to_string());
    info!("PDF successfully extracted using backend: {}", backend);

    let py_slides = result.slides.unwrap_or_default();
    let mut slides = Vec::new();

    for py_slide in py_slides {
        let mut elements = Vec::new();

        for py_el in py_slide.elements {
            let bbox = py_el.bbox.map(|(x0, y0, x1, y1)| BoundingBox::new(x0, y0, x1, y1));

            match py_el.element_type.as_str() {
                "title" => {
                    if let Some(t) = py_el.text {
                        if !t.trim().is_empty() {
                            elements.push(Element::Title { text: t, bbox });
                        }
                    }
                }
                "subtitle" => {
                    if let Some(t) = py_el.text {
                        if !t.trim().is_empty() {
                            elements.push(Element::Subtitle { text: t, bbox });
                        }
                    }
                }
                "section_header" => {
                    if let Some(t) = py_el.text {
                        if !t.trim().is_empty() {
                            elements.push(Element::SectionHeader { text: t, bbox });
                        }
                    }
                }
                "bullet_list" => {
                    // Accept items array OR fall back to splitting text lines
                    let raw_items: Vec<String> = py_el
                        .items
                        .filter(|v| !v.is_empty())
                        .or_else(|| {
                            py_el.text.as_ref().map(|t| {
                                t.lines()
                                    .map(|l| l.trim().to_string())
                                    .filter(|l| !l.is_empty())
                                    .collect()
                            })
                        })
                        .unwrap_or_default();

                    if !raw_items.is_empty() {
                        elements.push(Element::BulletList {
                            items: raw_items,
                            level: py_el.level,
                            bbox,
                        });
                    }
                }
                "table" => {
                    // Wire up the proper structured Table element (not markdown text).
                    let headers = py_el.headers.unwrap_or_default();
                    let rows = py_el.rows.unwrap_or_default();

                    // Only push if there is at least one header or one row of data.
                    if !headers.is_empty() || !rows.is_empty() {
                        elements.push(Element::Table {
                            headers,
                            rows,
                            bbox,
                        });
                    }
                }
                _ => {
                    // Default: plain text block; skip empty or whitespace-only strings.
                    if let Some(t) = py_el.text {
                        let trimmed = t.trim().to_string();
                        if !trimmed.is_empty() {
                            elements.push(Element::TextBlock { text: trimmed, bbox });
                        }
                    }
                }
            }
        }

        slides.push(Slide {
            slide_number: py_slide.slide_number,
            elements,
        });
    }

    Ok(slides)
}
