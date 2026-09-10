use std::path::{Path, PathBuf};

use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};

pub struct PdfSource;

/// Longest stored body text, in chars.
const MAX_BODY_CHARS: usize = 500_000;

/// Inline page marker written between extracted pages. HTML-comment syntax so
/// it stays inert if a body is ever rendered as markdown, and distinctive
/// enough to scan backwards for when resolving a match to its page.
pub const PAGE_MARKER_PREFIX: &str = "<!-- page ";

fn page_marker(n: usize) -> String {
    format!("{PAGE_MARKER_PREFIX}{n} -->")
}

/// Join per-page text with page markers so a stored body keeps its page
/// boundaries. Pages arrive in document order.
fn join_pages(pages: &[String]) -> String {
    let mut out = String::new();
    for (i, page) in pages.iter().enumerate() {
        out.push_str(&page_marker(i + 1));
        out.push('\n');
        out.push_str(page);
        if !page.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Recognized section headings (one per line) get `## ` markers so the stored
/// body keeps its skeleton and downstream readers can cite a section.
fn section_heading_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?mi)^\s*(abstract|introduction|background|related work|methods|method|materials and methods|results|results and discussion|discussion|conclusions?|references|acknowledg(?:e)?ments?)\s*:?\s*$")
            .unwrap()
    })
}

fn prepare_body(text: &str) -> Option<String> {
    let collapsed: String = text
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let marked = section_heading_re().replace_all(&collapsed, "\n## $1\n");
    if marked.trim().is_empty() {
        return None;
    }
    Some(marked.chars().take(MAX_BODY_CHARS).collect())
}

/// The per-page extractor stops at the first page it cannot render and
/// reports that as end-of-document, so a mid-document failure silently
/// truncates the body. Compare against the document's own page count and say
/// so rather than storing a short body as if it were complete.
fn warn_truncated(bytes: &[u8], pages: &[String], label: &str) {
    if let Some(expected) = load_page_count(bytes)
        && expected > pages.len()
    {
        eprintln!(
            "Warning: extracted {} of {expected} page(s) from {label}; the stored body is truncated.",
            pages.len()
        );
    }
}

/// pdf-extract panics (internal assertion failures) on some content streams
/// instead of returning Err — a single bad PDF must not kill ingest or
/// reingest, so catch the unwind and report it as an ordinary source error.
fn extract_pages(bytes: &[u8], label: &str) -> Result<Vec<String>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem_by_pages(bytes)
    })) {
        Ok(Ok(pages)) => Ok(pages),
        Ok(Err(e)) => Err(ResearchError::Source(format!(
            "PDF text extraction failed for {label}: {e}"
        ))),
        Err(p) => {
            let detail = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(ResearchError::Source(format!(
                "PDF text extraction panicked for {label}: {detail}"
            )))
        }
    }
}

/// Same guard for the metadata load behind the truncation warning.
fn load_page_count(bytes: &[u8]) -> Option<usize> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::Document::load_mem(bytes)
            .ok()
            .map(|d| d.get_pages().len())
    }))
    .unwrap_or(None)
}

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

    /// Extract page-anchored body text from PDF bytes, without creating a
    /// `Paper`. Used by the arXiv download path; `label` appears only in
    /// warnings.
    pub fn extract_body(&self, bytes: &[u8], label: &str) -> Result<Option<String>> {
        let pages = extract_pages(bytes, label)?;
        warn_truncated(bytes, &pages, label);
        Ok(prepare_body(&join_pages(&pages)))
    }

    pub fn ingest_file(&self, path: &Path) -> Result<(Paper, Option<String>)> {
        let bytes = std::fs::read(path)?;

        // Per-page extraction keeps page boundaries, which whole-document
        // extraction discards. Same engine underneath, so output is unchanged
        // apart from the markers.
        let pages = extract_pages(&bytes, &path.display().to_string())?;
        warn_truncated(&bytes, &pages, &path.display().to_string());
        let text = join_pages(&pages);

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

        // Keep the full body (section-marked, size-capped) so FTS5 and the
        // vector index can search inside the paper, not just its metadata.
        // ponytail: 500_000-char cap per paper — a real ceiling; raise if
        // book-length PDFs matter someday.
        let body = prepare_body(&text);
        Ok((paper, body))
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

    #[test]
    fn joins_pages_with_markers() {
        let joined = join_pages(&["first page".into(), "second page".into()]);
        assert!(joined.starts_with("<!-- page 1 -->\nfirst page"));
        assert!(joined.contains("<!-- page 2 -->\nsecond page"));
        // Every page contributes exactly one marker.
        assert_eq!(joined.matches(PAGE_MARKER_PREFIX).count(), 2);
    }

    /// Markers must survive section marking, since anchoring reads both.
    #[test]
    fn page_markers_survive_prepare_body() {
        let joined = join_pages(&["Introduction\ntext here".into(), "Results\nmore".into()]);
        let body = prepare_body(&joined).unwrap();
        assert!(body.contains("<!-- page 1 -->"));
        assert!(body.contains("<!-- page 2 -->"));
        assert!(body.contains("## Introduction"));
        assert!(body.contains("## Results"));
    }

    #[test]
    fn empty_page_list_yields_no_body() {
        assert!(prepare_body(&join_pages(&[])).is_none());
    }

    /// pdf-extract can panic on malformed content streams; whatever it does
    /// with garbage bytes, extraction must return instead of killing the
    /// process (regression net for the `catch_unwind` guards).
    #[test]
    fn garbage_pdf_yields_error_not_panic() {
        let bytes = b"%PDF-1.4\ntrailer\n<< /Root 1 0 R >>\nstartxref\n0\n%%EOF";
        match PdfSource::new().extract_body(bytes, "garbage") {
            Ok(None) | Err(_) => {}
            Ok(Some(body)) => assert!(!body.is_empty()),
        }
    }
}
