use anyhow::{bail, Result};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum FileType {
    PDF,
    PPTX,
    Unsupported,
}

pub fn detect_file_type(path: &Path) -> Result<FileType> {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let lower = ext.to_lowercase();
        if lower == "pdf" {
            return Ok(FileType::PDF);
        } else if lower == "pptx" {
            return Ok(FileType::PPTX);
        }
    }
    bail!("Unsupported file type. Must be .pdf or .pptx")
}
