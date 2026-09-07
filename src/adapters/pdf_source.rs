use std::path::{Path, PathBuf};

use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};

pub struct PdfSource;

impl PdfSource {
    pub fn new() -> Self {
        Self
    }

    /// Ingest all PDF files under `path` (file or directory).
    pub fn collect_paths(path: &Path) -> Result<Vec<PathBuf>> {
        if path.is_file() {
            return Ok(vec![path.to_path_buf()]);
        }
        if path.is_dir() {
            let mut paths = Vec::new();
            for entry in std::fs::read_dir(path)? {
                let p = entry?.path();
                if p.extension().and_then(|e| e.to_str()) == Some("pdf") {
                    paths.push(p);
                }
            }
            paths.sort();
            return Ok(paths);
        }
        Err(ResearchError::Source(format!(
            "Path does not exist: {}",
            path.display()
        )))
    }

    pub fn ingest_file(&self, path: &Path) -> Result<Paper> {
        let bytes = std::fs::read(path)?;

        let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| {
            ResearchError::Source(format!(
                "PDF text extraction failed for {}: {e}",
                path.display()
            ))
        })?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled PDF")
            .to_string();

        let mut paper = Paper::new(title);
        paper.abstract_text = text
            .lines()
            .take(30)
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(1000)
            .collect();
        paper.pdf_path = Some(
            path.canonicalize()
                .unwrap_or_else(|_| path.to_path_buf())
                .display()
                .to_string(),
        );

        Ok(paper)
    }

    pub fn name(&self) -> &str {
        "pdf"
    }
}

impl Default for PdfSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn collect_paths_single_file() {
        let dir = tempdir().unwrap();
        let pdf = dir.path().join("paper.pdf");
        std::fs::write(&pdf, b"%PDF-1.4 fake").unwrap();
        let paths = PdfSource::collect_paths(&pdf).unwrap();
        assert_eq!(paths, vec![pdf]);
    }

    #[test]
    fn collect_paths_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), b"%PDF-1.4 fake").unwrap();
        std::fs::write(dir.path().join("b.pdf"), b"%PDF-1.4 fake").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"text").unwrap();
        let paths = PdfSource::collect_paths(dir.path()).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| p.extension().unwrap() == "pdf"));
    }

    #[test]
    fn collect_paths_nonexistent_errors() {
        let result = PdfSource::collect_paths(Path::new("/nonexistent/path/to/file.pdf"));
        assert!(result.is_err());
    }

    #[test]
    fn source_name() {
        assert_eq!(PdfSource::new().name(), "pdf");
    }
}
