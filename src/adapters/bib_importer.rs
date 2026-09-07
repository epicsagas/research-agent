//! BibTeX/BibLaTeX and CSL-JSON parsing → `Paper` records.
//!
//! The import entry point lives in `application::paper_import`; this module is
//! pure parsing (no I/O) so tests run offline over string fixtures. Zotero
//! users export from Zotero as BibTeX or CSL JSON and import the files.

use biblatex::ChunksExt;

use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};

/// Best-effort publication year from the `date`/`year` field (ranges take the
/// start year; literal string dates yield None).
fn entry_year(entry: &biblatex::Entry) -> Option<u32> {
    match entry.date().ok()? {
        biblatex::PermissiveType::Typed(date) => {
            let year = match date.value {
                biblatex::DateValue::At(dt) => Some(dt.year),
                biblatex::DateValue::After(dt) | biblatex::DateValue::Before(dt) => Some(dt.year),
                biblatex::DateValue::Between(a, _) => Some(a.year),
            }?;
            u32::try_from(year).ok()
        }
        biblatex::PermissiveType::Chunks(_) => None,
    }
}

/// Parse a `.bib`/`.bibtex` file into papers. Entries without a title are
/// skipped; every other field is best-effort.
pub fn parse_bibtex(content: &str) -> Result<Vec<Paper>> {
    let bib = biblatex::Bibliography::parse(content)
        .map_err(|e| ResearchError::Source(format!("BibTeX parse failed: {e}")))?;

    let mut papers = Vec::new();
    for entry in bib.iter() {
        let Ok(title) = entry.title().map(|t| t.format_verbatim()) else {
            continue;
        };
        if title.trim().is_empty() {
            continue;
        }
        let mut paper = Paper::new(title);

        if let Ok(authors) = entry.author() {
            paper.authors = authors
                .iter()
                .map(|p| {
                    [
                        p.given_name.trim(),
                        p.prefix.trim(),
                        p.name.trim(),
                        p.suffix.trim(),
                    ]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
                })
                .collect();
        }
        paper.year = entry_year(entry);
        paper.doi = entry.doi().ok().filter(|s| !s.is_empty());
        paper.url = entry.url().ok().filter(|s| !s.is_empty());
        paper.venue = entry
            .journal()
            .or_else(|_| entry.book_title())
            .ok()
            .map(|v| v.format_verbatim())
            .filter(|v| !v.is_empty());
        papers.push(paper);
    }
    Ok(papers)
}

#[derive(serde::Deserialize)]
struct CslItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "container-title")]
    container_title: Option<String>,
    #[serde(default, rename = "DOI")]
    doi: Option<String>,
    #[serde(default, rename = "URL")]
    url: Option<String>,
    #[serde(default)]
    issued: Option<CslIssued>,
    #[serde(default)]
    author: Vec<CslAuthor>,
}

#[derive(serde::Deserialize)]
struct CslIssued {
    #[serde(default, rename = "date-parts")]
    date_parts: Vec<Vec<Option<i64>>>,
}

#[derive(serde::Deserialize)]
struct CslAuthor {
    #[serde(default)]
    given: Option<String>,
    #[serde(default)]
    family: Option<String>,
}

/// Parse a CSL-JSON file (Zotero's "CSL JSON" export format) into papers.
pub fn parse_csl_json(content: &str) -> Result<Vec<Paper>> {
    let items: Vec<CslItem> = serde_json::from_str(content)
        .map_err(|e| ResearchError::Source(format!("CSL-JSON parse failed: {e}")))?;

    Ok(items
        .into_iter()
        .filter_map(|item| {
            let title = item.title.filter(|t| !t.trim().is_empty())?;
            let mut paper = Paper::new(title);
            paper.venue = item.container_title.filter(|v| !v.is_empty());
            paper.doi = item.doi.filter(|d| !d.is_empty());
            paper.url = item.url.filter(|u| !u.is_empty());
            paper.year = item
                .issued
                .and_then(|i| {
                    i.date_parts
                        .first()
                        .and_then(|parts| parts.first().copied())
                })
                .and_then(|y| y) // date-parts entries are optional: [[2013]]
                .filter(|y| *y > 0)
                .map(|y| y as u32);
            paper.authors = item
                .author
                .iter()
                .map(|a| {
                    [
                        a.given.as_deref().unwrap_or(""),
                        a.family.as_deref().unwrap_or(""),
                    ]
                    .join(" ")
                    .trim()
                    .to_string()
                })
                .filter(|a| !a.is_empty())
                .collect();
            Some(paper)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIB_FIXTURE: &str = r#"
        @article{kucsko2013,
            title = {Nanometre-scale thermometry in a living cell},
            author = {Kucsko, Georg and Maurer, Peter C.},
            year = {2013},
            journal = {Nature},
            doi = {10.1038/nature12373},
            url = {https://www.nature.com/articles/nature12373}
        }
        @book{no-title-entry,
            author = {Nobody}
        }
    "#;

    #[test]
    fn parses_bibtex_entry() {
        let papers = parse_bibtex(BIB_FIXTURE).unwrap();
        assert_eq!(papers.len(), 1, "title-less entries are skipped");
        let paper = &papers[0];
        assert_eq!(paper.title, "Nanometre-scale thermometry in a living cell");
        assert_eq!(paper.authors, vec!["Georg Kucsko", "Peter C. Maurer"]);
        assert_eq!(paper.year, Some(2013));
        assert_eq!(paper.venue.as_deref(), Some("Nature"));
        assert_eq!(paper.doi.as_deref(), Some("10.1038/nature12373"));
    }

    #[test]
    fn bibtex_garbage_yields_no_papers() {
        // biblatex is lenient: junk outside entries is skipped, not an error.
        assert!(parse_bibtex("this is not bibtex {").unwrap().is_empty());
    }

    const CSL_FIXTURE: &str = r#"[
        {
            "id": "kucsko2013",
            "type": "article-journal",
            "title": "Nanometre-scale thermometry in a living cell",
            "container-title": "Nature",
            "DOI": "10.1038/nature12373",
            "URL": "https://www.nature.com/articles/nature12373",
            "issued": {"date-parts": [[2013, 7, 31]]},
            "author": [
                {"given": "Georg", "family": "Kucsko"},
                {"given": "Peter C.", "family": "Maurer"}
            ]
        },
        {"id": "no-title", "type": "book"}
    ]"#;

    #[test]
    fn parses_csl_json() {
        let papers = parse_csl_json(CSL_FIXTURE).unwrap();
        assert_eq!(papers.len(), 1, "title-less items are skipped");
        let paper = &papers[0];
        assert_eq!(paper.title, "Nanometre-scale thermometry in a living cell");
        assert_eq!(paper.authors, vec!["Georg Kucsko", "Peter C. Maurer"]);
        assert_eq!(paper.year, Some(2013));
        assert_eq!(paper.venue.as_deref(), Some("Nature"));
        assert_eq!(paper.doi.as_deref(), Some("10.1038/nature12373"));
    }

    #[test]
    fn csl_parse_error_is_typed() {
        assert!(parse_csl_json("not json").is_err());
    }
}
