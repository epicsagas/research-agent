//! File import pipeline: BibTeX/BibLaTeX, CSL-JSON, or Zotero-native JSON
//! files → store, with DOI de-duplication. Shared by the `research import`
//! CLI command and the MCP `import_papers` tool.

use std::path::{Path, PathBuf};

use crate::adapters::bib_importer::{parse_bibtex, parse_json_auto};
use crate::application::identity::is_already_stored;
use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::index_store::IndexStore;

/// Outcome of one import run. `failed` carries human-readable per-file errors
/// so one bad file never blocks the rest of the batch.
pub struct ImportSummary {
    pub imported: Vec<Paper>,
    pub skipped_duplicates: usize,
    pub failed: Vec<String>,
}

const IMPORT_EXTENSIONS: [&str; 3] = ["bib", "bibtex", "json"];

/// Import a file or a directory of files into the store.
pub fn run_import(store: &dyn IndexStore, path: &Path) -> Result<ImportSummary> {
    let files = collect_files(path)?;
    let mut summary = ImportSummary {
        imported: Vec::new(),
        skipped_duplicates: 0,
        failed: Vec::new(),
    };

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                summary.failed.push(format!("{}: {e}", file.display()));
                continue;
            }
        };
        let ext = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let parsed = match ext.as_str() {
            "bib" | "bibtex" => parse_bibtex(&content),
            // Both CSL-JSON and Zotero's native export are `.json`; the
            // parser is picked by content, not by extension.
            "json" => parse_json_auto(&content),
            other => Err(ResearchError::Source(format!(
                "unsupported import format: .{other} (use .bib or .json)"
            ))),
        };
        let papers = match parsed {
            Ok(papers) => papers,
            Err(e) => {
                summary.failed.push(format!("{}: {e}", file.display()));
                continue;
            }
        };
        for paper in papers {
            if is_already_stored(store, &paper)? {
                summary.skipped_duplicates += 1;
                continue;
            }
            store.insert_paper(&paper)?;
            summary.imported.push(paper);
        }
    }
    Ok(summary)
}

/// `path` is one file or a directory scanned non-recursively for
/// `.bib`/`.bibtex`/`.json` files (sorted for deterministic order).
fn collect_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if path.is_dir() {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let p = entry?.path();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if ext.is_some_and(|e| IMPORT_EXTENSIONS.contains(&e.as_str())) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_store::SqliteStore;
    use tempfile::tempdir;

    #[test]
    fn imports_bib_and_skips_doi_duplicates() {
        let dir = tempdir().unwrap();
        let bib = dir.path().join("refs.bib");
        std::fs::write(
            &bib,
            r#"
            @article{a, title = {Alpha Paper}, author = {A. One}, doi = {10.1/alpha}}
            @article{b, title = {Beta Paper}, author = {B. Two}}
        "#,
        )
        .unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let summary = run_import(&store, dir.path()).unwrap();
        assert_eq!(summary.imported.len(), 2);
        assert_eq!(summary.skipped_duplicates, 0);
        assert!(summary.failed.is_empty());

        // Second run: every entry is a duplicate now — by DOI, and the
        // DOI-less one by title.
        let summary = run_import(&store, dir.path()).unwrap();
        assert_eq!(
            summary.imported.len(),
            0,
            "the DOI-less paper is caught by its title"
        );
        assert_eq!(summary.skipped_duplicates, 2);
    }

    #[test]
    fn bad_file_is_reported_not_fatal() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("good.bib"), "@article{c, title = {Good}}").unwrap();
        std::fs::write(dir.path().join("broken.bib"), "@article{d, title = {").unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let summary = run_import(&store, dir.path()).unwrap();
        assert_eq!(summary.imported.len(), 1);
        assert_eq!(summary.failed.len(), 1);
        assert!(summary.failed[0].contains("broken.bib"));
    }

    #[test]
    fn nonexistent_path_errors() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(run_import(&store, Path::new("/nonexistent/refs.bib")).is_err());
    }
}
