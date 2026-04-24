use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

/// Extract images from a PDF using `pdfimages` and OCR them with Tesseract.
///
/// This module provides image-based content extraction by:
/// 1. Running `pdfimages` to extract embedded images from PDFs
/// 2. Running `tesseract` on each extracted image to get text
/// 3. Returning all OCR'd text associated with page numbers
///
/// Both `pdfimages` and `tesseract` must be available in the system PATH.
pub struct OcrExtractor;

/// Result of OCR extraction for a single image.
#[derive(Debug, Clone)]
pub struct OcrResult {
    /// The page number the image was found on (if available)
    pub page_number: Option<u32>,
    /// The OCR'd text content
    pub text: String,
    /// Source image filename
    pub source_image: String,
}

impl OcrExtractor {
    /// Check if Tesseract OCR is available on the system.
    pub async fn is_available() -> bool {
        Command::new("tesseract")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if pdfimages is available on the system.
    pub async fn pdfimages_available() -> bool {
        Command::new("pdfimages")
            .arg("-v")
            .output()
            .await
            .map(|_| true) // pdfimages -v exits with 1 but still means it's installed
            .unwrap_or(false)
    }

    /// Detect whether installed `pdfimages` supports Poppler-style flags (-png, -p).
    async fn pdfimages_supports_poppler_flags() -> bool {
        let output = match Command::new("pdfimages").arg("--help").output().await {
            Ok(o) => o,
            Err(_) => return false,
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        let help = format!("{}\n{}", stdout, stderr);
        help.contains("-png") && help.contains("-p")
    }

    /// Extract images from a PDF and OCR them.
    ///
    /// Returns a list of `OcrResult` containing the text found in each image.
    /// If pdfimages or tesseract is not available, returns an empty list with a warning.
    pub async fn extract_from_pdf(pdf_path: &Path) -> Result<Vec<OcrResult>> {
        if !Self::pdfimages_available().await {
            warn!("pdfimages not found in PATH; skipping image extraction");
            return Ok(vec![]);
        }
        if !Self::is_available().await {
            warn!("tesseract not found in PATH; skipping OCR. Install Tesseract to enable image text extraction.");
            return Ok(vec![]);
        }

        info!("Extracting images from PDF for OCR: {:?}", pdf_path);

        // Create a temporary directory for extracted images
        let tmp_dir = std::env::temp_dir().join(format!("ocr_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).context("Failed to create temp directory for OCR")?;

        let tmp_prefix = tmp_dir.join("img");

        // Run pdfimages with compatible arguments for the installed distribution.
        let supports_poppler = Self::pdfimages_supports_poppler_flags().await;
        let output = if supports_poppler {
            Command::new("pdfimages")
                .arg("-png")
                .arg("-p")
                .arg(pdf_path)
                .arg(&tmp_prefix)
                .output()
                .await
                .context("Failed to run pdfimages (Poppler mode)")?
        } else {
            info!("pdfimages does not support -png/-p; using basic extraction mode");
            Command::new("pdfimages")
                .arg(pdf_path)
                .arg(&tmp_prefix)
                .output()
                .await
                .context("Failed to run pdfimages (basic mode)")?
        };

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            warn!("pdfimages exited with error: {}", err);
            cleanup_tmp(&tmp_dir);
            return Ok(vec![]);
        }

        // Find extracted image files
        let image_files: Vec<_> = std::fs::read_dir(&tmp_dir)
            .context("Failed to read temp directory")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "png" || ext == "jpg" || ext == "ppm")
                    .unwrap_or(false)
            })
            .collect();

        if image_files.is_empty() {
            info!("No images found in PDF");
            cleanup_tmp(&tmp_dir);
            return Ok(vec![]);
        }

        info!("Found {} images, running OCR...", image_files.len());

        let mut results = Vec::new();

        for entry in &image_files {
            let img_path = entry.path();
            let filename = img_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Check image dimensions and skip tiny elements like icons or logos (< 100x100 px)
            if let Ok(dimensions) = image::image_dimensions(&img_path) {
                if dimensions.0 < 100 || dimensions.1 < 100 {
                    info!(
                        "Skipping small image {} ({}x{})",
                        filename, dimensions.0, dimensions.1
                    );
                    continue;
                }
            }

            // Try to extract page number from filename (format: img-NNN-MMM.png)
            let page_number = filename
                .split('-')
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok());

            // Run Tesseract on the image
            match ocr_image(&img_path).await {
                Ok(text) => {
                    if !text.trim().is_empty() {
                        results.push(OcrResult {
                            page_number,
                            text: text.trim().to_string(),
                            source_image: filename,
                        });
                    }
                }
                Err(e) => {
                    warn!("OCR failed for {}: {}", filename, e);
                }
            }
        }

        // Cleanup temp directory
        cleanup_tmp(&tmp_dir);

        info!("OCR complete: {} images yielded text", results.len());
        Ok(results)
    }

    /// Extract images from a PPTX file and OCR them.
    ///
    /// PPTX images are stored in `ppt/media/` inside the ZIP archive.
    pub async fn extract_from_pptx(pptx_path: &Path) -> Result<Vec<OcrResult>> {
        if !Self::is_available().await {
            warn!("tesseract not found in PATH; skipping PPTX image OCR");
            return Ok(vec![]);
        }

        info!("Extracting images from PPTX for OCR: {:?}", pptx_path);

        let file = std::fs::File::open(pptx_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .context("Failed to open PPTX as zip for image extraction")?;

        let tmp_dir = std::env::temp_dir().join(format!("ocr_pptx_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir)?;

        let mut image_entries: Vec<String> = Vec::new();

        // Collect image file names
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if name.starts_with("ppt/media/") && is_image_extension(&name) {
                    image_entries.push(name);
                }
            }
        }

        let mut results = Vec::new();

        for name in &image_entries {
            let mut img_data = Vec::new();
            let img_filename = Path::new(name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            {
                if let Ok(mut entry) = archive.by_name(name) {
                    let _ = std::io::Read::read_to_end(&mut entry, &mut img_data);
                }
            }

            if img_data.is_empty() {
                continue;
            }

            let img_path = tmp_dir.join(&img_filename);
            if std::fs::write(&img_path, &img_data).is_ok() {
                // Check dimensions to skip icons/logos
                if let Ok(dimensions) = image::image_dimensions(&img_path) {
                    if dimensions.0 < 100 || dimensions.1 < 100 {
                        info!(
                            "Skipping small PPTX image {} ({}x{})",
                            img_filename, dimensions.0, dimensions.1
                        );
                        continue;
                    }
                }

                if let Ok(text) = ocr_image(&img_path).await {
                    if !text.trim().is_empty() {
                        results.push(OcrResult {
                            page_number: None,
                            text: text.trim().to_string(),
                            source_image: img_filename,
                        });
                    }
                }
            }
        }

        cleanup_tmp(&tmp_dir);

        info!("PPTX OCR complete: {} images yielded text", results.len());
        Ok(results)
    }
}

/// Run Tesseract OCR on a single image file.
async fn ocr_image(image_path: &Path) -> Result<String> {
    let output = Command::new("tesseract")
        .arg(image_path)
        .arg("stdout") // Output to stdout instead of file
        .arg("--dpi")
        .arg("300") // Improve accuracy by hinting resolution
        .arg("--psm")
        .arg("3") // Page segmentation mode: fully automatic page segmentation
        .output()
        .await
        .context("Failed to run tesseract")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Tesseract failed: {}", err);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check if a filename has a common image extension.
fn is_image_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".tiff")
        || lower.ends_with(".bmp")
        || lower.ends_with(".emf")
        || lower.ends_with(".wmf")
}

/// Clean up a temporary directory silently.
fn cleanup_tmp(dir: &Path) {
    if let Err(e) = std::fs::remove_dir_all(dir) {
        warn!("Failed to cleanup temp dir {:?}: {}", dir, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_extension() {
        assert!(is_image_extension("image001.png"));
        assert!(is_image_extension("photo.jpg"));
        assert!(is_image_extension("scan.JPEG"));
        assert!(!is_image_extension("document.xml"));
        assert!(!is_image_extension("slide.rs"));
    }
}
