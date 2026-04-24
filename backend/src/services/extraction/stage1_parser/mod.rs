pub mod file_type;
pub mod ocr;
pub mod pdf;
pub mod pptx;

use crate::models::extraction_model::{Element, Slide};
use anyhow::Result;
use std::path::Path;
use tracing::info;

/// Parse a document (PDF or PPTX) into slides.
///
/// This dispatcher:
/// 1. Determines file type from extension
/// 2. Delegates to the appropriate parser (pdf.rs or pptx.rs)
/// 3. Runs OCR on embedded images to capture image-based text
/// 4. Merges OCR results into the corresponding slides
pub async fn parse_document(path: &Path, extension: &str) -> Result<Vec<Slide>> {
    let ext = if extension.is_empty() {
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
    } else {
        extension.to_lowercase()
    };

    let mut slides = match ext.as_str() {
        "pdf" => {
            let mut pdf_slides = pdf::parse_pdf(path).await?;

            // Run OCR on embedded images in the PDF
            let ocr_results = ocr::OcrExtractor::extract_from_pdf(path)
                .await
                .unwrap_or_default();
            if !ocr_results.is_empty() {
                info!(
                    "OCR extracted text from {} images in PDF",
                    ocr_results.len()
                );
                merge_ocr_results(&mut pdf_slides, ocr_results);
            }

            pdf_slides
        }
        "pptx" => {
            let mut pptx_slides = pptx::parse_pptx(path)?;

            // Run OCR on embedded images in the PPTX
            let ocr_results = ocr::OcrExtractor::extract_from_pptx(path)
                .await
                .unwrap_or_default();
            if !ocr_results.is_empty() {
                info!(
                    "OCR extracted text from {} images in PPTX",
                    ocr_results.len()
                );
                merge_ocr_results(&mut pptx_slides, ocr_results);
            }

            pptx_slides
        }
        _ => anyhow::bail!("Unsupported file type: {}", ext),
    };

    // Filter out completely empty slides
    slides.retain(|s| !s.elements.is_empty());
    info!(
        "Stage 1 extraction complete: {} slides processed",
        slides.len()
    );

    Ok(slides)
}

/// Merge OCR results into the appropriate slides.
///
/// If an OCR result has a page number, it is added to that specific slide.
/// Otherwise, OCR text is appended to the last slide.
fn merge_ocr_results(slides: &mut [Slide], ocr_results: Vec<ocr::OcrResult>) {
    for result in ocr_results {
        let element = Element::Image {
            path: std::path::PathBuf::from(&result.source_image),
            bbox: None, // We don't have accurate bboxes for embedded images yet
            ocr_text: Some(result.text),
        };

        if let Some(page) = result.page_number {
            // Find the matching slide by number (pdfpages prefix is usually 1-based)
            if let Some(slide) = slides.iter_mut().find(|s| s.slide_number == page) {
                slide.elements.push(element);
            } else if let Some(last) = slides.last_mut() {
                last.elements.push(element);
            }
        } else if let Some(last) = slides.last_mut() {
            last.elements.push(element);
        }
    }
}
