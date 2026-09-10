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

/// Normalize a DOI for storage and dedupe: strip the resolver prefix, trim,
/// lowercase. DOIs are case-insensitive by spec, and the exact form varies by
/// exporter, so `10.1/X`, `https://doi.org/10.1/X`, and `10.1/x` are one paper.
pub fn normalize_doi(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("https://doi.org/")
        .or_else(|| trimmed.strip_prefix("http://doi.org/"))
        .or_else(|| trimmed.strip_prefix("http://dx.doi.org/"))
        .or_else(|| trimmed.strip_prefix("https://dx.doi.org/"))
        .or_else(|| trimmed.strip_prefix("doi:"))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        return None;
    }
    Some(stripped.to_ascii_lowercase())
}

/// Split an exported keyword field into tags. Zotero writes comma-separated
/// keywords; BibTeX files in the wild also use semicolons.
fn split_keywords(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
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
        paper.doi = entry.doi().ok().and_then(|d| normalize_doi(&d));
        paper.url = entry.url().ok().filter(|s| !s.is_empty());
        // Exports carry these; dropping them left imported papers empty for
        // gap analysis and semantic search, which read `abstract_text`.
        if let Some(chunks) = entry.fields.get("abstract") {
            let text = chunks.format_verbatim();
            if !text.trim().is_empty() {
                paper.abstract_text = text;
            }
        }
        if let Some(chunks) = entry.fields.get("keywords") {
            paper.tags = split_keywords(&chunks.format_verbatim());
        }
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
    #[serde(default, rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(default)]
    keyword: Option<String>,
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

/// A CSL-JSON date part, which the schema allows as a number or a string —
/// Zotero's CSL export emits `[["2023"]]`, not `[[2023]]`.
#[derive(serde::Deserialize, Clone)]
#[serde(untagged)]
enum CslYearPart {
    Number(i64),
    Text(String),
}

impl CslYearPart {
    fn to_year(&self) -> Option<i64> {
        match self {
            CslYearPart::Number(n) => Some(*n),
            CslYearPart::Text(s) => s.trim().parse().ok(),
        }
    }
}

#[derive(serde::Deserialize)]
struct CslIssued {
    #[serde(default, rename = "date-parts")]
    date_parts: Vec<Vec<Option<CslYearPart>>>,
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
            paper.doi = item.doi.as_deref().and_then(normalize_doi);
            paper.url = item.url.filter(|u| !u.is_empty());
            paper.abstract_text = item
                .abstract_text
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_default();
            paper.tags = item
                .keyword
                .as_deref()
                .map(split_keywords)
                .unwrap_or_default();
            paper.year = item
                .issued
                .and_then(|i| {
                    i.date_parts
                        .first()
                        .and_then(|parts| parts.first().cloned())
                })
                .and_then(|y| y) // date-parts entries are optional: [[2013]]
                .and_then(|y| y.to_year())
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

/// One item from Zotero's own JSON export (the "Zotero JSON" option in the
/// export dialog), which is not CSL-JSON: it nests names under `creators` and
/// calls the abstract `abstractNote`. The same item shape arrives nested under
/// `data` in local-API responses, so `zotero_source` reuses the mapping.
#[derive(serde::Deserialize, Default)]
pub(crate) struct ZoteroItem {
    #[serde(default, rename = "itemType")]
    item_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "abstractNote")]
    abstract_note: Option<String>,
    #[serde(default, rename = "publicationTitle")]
    publication_title: Option<String>,
    #[serde(default, rename = "DOI")]
    doi: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    creators: Vec<ZoteroCreator>,
    #[serde(default)]
    tags: Vec<ZoteroTag>,
}

impl ZoteroItem {
    /// Normalized DOI, if present. Shared by the read mapping and the write
    /// path's matching so both see the same identifier.
    pub(crate) fn normalized_doi(&self) -> Option<String> {
        self.doi.as_deref().and_then(normalize_doi)
    }

    /// The item's tags as plain strings, empty and untagged entries dropped.
    pub(crate) fn tag_strings(&self) -> Vec<String> {
        self.tags.iter().filter_map(|t| t.tag.clone()).collect()
    }
}

#[derive(serde::Deserialize)]
struct ZoteroCreator {
    #[serde(default, rename = "firstName")]
    first_name: Option<String>,
    #[serde(default, rename = "lastName")]
    last_name: Option<String>,
    /// Single-field creators (institutions) use `name` instead of first/last.
    #[serde(default)]
    name: Option<String>,
}

#[derive(serde::Deserialize)]
struct ZoteroTag {
    #[serde(default)]
    tag: Option<String>,
}

/// First 4-digit run in a Zotero `date` string, which is free-form
/// ("2013-08-01", "August 2013", "2013").
fn year_from_date_string(date: &str) -> Option<u32> {
    let bytes = date.as_bytes();
    let mut run = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            run += 1;
            if run == 4 {
                let start = i - 3;
                // Reject longer digit runs (e.g. a 5-digit id) by checking the
                // char just past the run.
                if bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                    run = 0;
                    continue;
                }
                return date[start..=i].parse().ok().filter(|y| *y > 0);
            }
        } else {
            run = 0;
        }
    }
    None
}

/// Parse Zotero's native JSON into papers. Desktop exports hold the item
/// fields at the top level; local/Web-API responses nest them under `data`.
pub fn parse_zotero_json(content: &str) -> Result<Vec<Paper>> {
    let values: Vec<serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| ResearchError::Source(format!("Zotero JSON parse failed: {e}")))?;
    // A type-mismatched item (e.g. `title` holding a number) must not turn
    // into an empty default item and vanish: report the failure instead of
    // silently importing fewer papers than the file contains.
    let mut failed = 0usize;
    let items = values
        .into_iter()
        .filter_map(|v| {
            let v = match v.get("data") {
                Some(data) => data.clone(),
                None => v,
            };
            match serde_json::from_value::<ZoteroItem>(v) {
                Ok(item) => Some(item),
                Err(_) => {
                    failed += 1;
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    if failed > 0 {
        return Err(ResearchError::Source(format!(
            "Zotero JSON: {failed} item(s) failed to parse"
        )));
    }
    Ok(papers_from_zotero_items(items))
}

/// Map Zotero item objects into papers. Shared by the file-export path
/// (`parse_zotero_json`, flat items) and the local-API path (`zotero_source`,
/// items nested under `data`), so field handling lives in exactly one place.
pub(crate) fn papers_from_zotero_items(items: Vec<ZoteroItem>) -> Vec<Paper> {
    items
        .into_iter()
        .filter(|i| {
            // Attachments and notes are not papers; they ride along in exports.
            !matches!(i.item_type.as_deref(), Some("attachment") | Some("note"))
        })
        .filter_map(|item| {
            let title = item.title.filter(|t| !t.trim().is_empty())?;
            let mut paper = Paper::new(title);
            paper.venue = item.publication_title.filter(|v| !v.is_empty());
            paper.doi = item.doi.as_deref().and_then(normalize_doi);
            paper.url = item.url.filter(|u| !u.is_empty());
            paper.abstract_text = item
                .abstract_note
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_default();
            paper.year = item.date.as_deref().and_then(year_from_date_string);
            paper.authors = item
                .creators
                .iter()
                .map(|c| match &c.name {
                    Some(n) => n.trim().to_string(),
                    None => [
                        c.first_name.as_deref().unwrap_or("").trim(),
                        c.last_name.as_deref().unwrap_or("").trim(),
                    ]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" "),
                })
                .filter(|a| !a.is_empty())
                .collect();
            paper.tags = item
                .tags
                .iter()
                .filter_map(|t| t.tag.as_deref())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            Some(paper)
        })
        .collect()
}

/// Pick the right JSON parser for `content`. Zotero's native JSON marks each
/// item with `itemType` (top level in desktop exports, under `data` in API
/// responses); CSL-JSON uses `type`. Both are `.json` on disk, so dispatching
/// on the extension alone fails whichever one it did not pick.
pub fn parse_json_auto(content: &str) -> Result<Vec<Paper>> {
    let looks_zotero = serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| {
            v.as_array().and_then(|a| a.first()).map(|first| {
                first.get("itemType").is_some()
                    || first.get("data").and_then(|d| d.get("itemType")).is_some()
            })
        })
        .unwrap_or(false);
    if looks_zotero {
        parse_zotero_json(content)
    } else {
        parse_csl_json(content)
    }
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

    #[test]
    fn normalizes_doi_forms_to_one_key() {
        // Resolver prefixes and case both vary by exporter; all three of these
        // name the same paper.
        let expected = Some("10.1038/nature12373".to_string());
        assert_eq!(normalize_doi("10.1038/nature12373"), expected);
        assert_eq!(
            normalize_doi("https://doi.org/10.1038/NATURE12373"),
            expected
        );
        assert_eq!(normalize_doi("  doi:10.1038/Nature12373 "), expected);
        assert_eq!(normalize_doi("   "), None);
    }

    #[test]
    fn splits_keywords_on_commas_and_semicolons() {
        assert_eq!(
            split_keywords("quantum sensing, diamond;  NV centers "),
            vec!["quantum sensing", "diamond", "NV centers"]
        );
        assert!(split_keywords(" , ; ").is_empty());
    }

    #[test]
    fn bibtex_keeps_abstract_and_keywords() {
        let bib = r#"
            @article{k2013,
                title = {Thermometry in a living cell},
                author = {Kucsko, Georg},
                doi = {https://doi.org/10.1038/NATURE12373},
                abstract = {We report nanoscale thermometry.},
                keywords = {quantum sensing, diamond}
            }
        "#;
        let papers = parse_bibtex(bib).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].abstract_text, "We report nanoscale thermometry.");
        assert_eq!(papers[0].tags, vec!["quantum sensing", "diamond"]);
        // Normalized on the way in, so it dedupes against other spellings.
        assert_eq!(papers[0].doi.as_deref(), Some("10.1038/nature12373"));
    }

    #[test]
    fn csl_keeps_abstract_and_keywords() {
        let csl = r#"[{
            "type": "article-journal",
            "title": "Thermometry in a living cell",
            "abstract": "We report nanoscale thermometry.",
            "keyword": "quantum sensing, diamond",
            "DOI": "10.1038/NATURE12373"
        }]"#;
        let papers = parse_csl_json(csl).unwrap();
        assert_eq!(papers[0].abstract_text, "We report nanoscale thermometry.");
        assert_eq!(papers[0].tags, vec!["quantum sensing", "diamond"]);
        assert_eq!(papers[0].doi.as_deref(), Some("10.1038/nature12373"));
    }

    /// Zotero's CSL export writes date parts as strings, which the CSL-JSON
    /// schema allows: `"issued": {"date-parts": [["2023"]]}`. Integer-only
    /// deserialization rejected every real Zotero export outright.
    #[test]
    fn csl_accepts_zotero_string_date_parts() {
        let csl = r#"[{
            "type": "article-journal",
            "title": "Attention Is All You Need",
            "issued": {"date-parts": [["2023", "8"]]}
        }, {
            "type": "book",
            "title": "Ancient text",
            "issued": {"date-parts": [["circa 1859"]]}
        }]"#;
        let papers = parse_csl_json(csl).unwrap();
        assert_eq!(papers[0].year, Some(2023));
        // Non-numeric parts yield no year instead of failing the whole file.
        assert_eq!(papers[1].year, None);
    }

    #[test]
    fn parses_zotero_native_json() {
        let zot = r#"[{
            "itemType": "journalArticle",
            "title": "Thermometry in a living cell",
            "abstractNote": "We report nanoscale thermometry.",
            "publicationTitle": "Nature",
            "DOI": "10.1038/nature12373",
            "date": "2013-08-01",
            "creators": [
                {"firstName": "Georg", "lastName": "Kucsko"},
                {"name": "Some Institute"}
            ],
            "tags": [{"tag": "quantum sensing"}, {"tag": "diamond"}]
        }, {
            "itemType": "attachment",
            "title": "Full Text PDF"
        }]"#;
        let papers = parse_zotero_json(zot).unwrap();
        // The attachment rides along in real exports and is not a paper.
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].venue.as_deref(), Some("Nature"));
        assert_eq!(papers[0].year, Some(2013));
        assert_eq!(papers[0].authors, vec!["Georg Kucsko", "Some Institute"]);
        assert_eq!(papers[0].tags, vec!["quantum sensing", "diamond"]);
    }

    /// `.json` covers two different formats, so the parser is chosen by
    /// content. Picking by extension alone broke whichever one it did not pick.
    #[test]
    fn json_dispatch_picks_parser_by_content() {
        let zot = r#"[{"itemType": "journalArticle", "title": "Z"}]"#;
        let csl = r#"[{"type": "article-journal", "title": "C"}]"#;
        assert_eq!(parse_json_auto(zot).unwrap()[0].title, "Z");
        assert_eq!(parse_json_auto(csl).unwrap()[0].title, "C");
        assert!(parse_json_auto("not json").is_err());
    }

    /// Local/Web-API responses nest the item fields under `data`. The
    /// flat-only parser accepted such files but silently produced zero papers
    /// from them.
    #[test]
    fn zotero_api_json_with_data_wrapper_imports() {
        let api = r#"[{
            "key": "93929HEK",
            "version": 50,
            "library": {"type": "user", "id": 1, "name": "My Library"},
            "meta": {"creatorSummary": "K. He"},
            "data": {
                "key": "93929HEK",
                "itemType": "conferencePaper",
                "title": "Deep Residual Learning for Image Recognition",
                "abstractNote": "Deeper neural networks are more difficult.",
                "DOI": "10.1109/CVPR.2016.90",
                "date": "6/2016",
                "creators": [{"firstName": "Kaiming", "lastName": "He"}],
                "tags": [{"tag": "resnet"}]
            }
        }, {
            "key": "ATTACH1",
            "data": {"key": "ATTACH1", "itemType": "attachment", "title": "PDF"}
        }]"#;
        let papers = parse_zotero_json(api).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(
            papers[0].title,
            "Deep Residual Learning for Image Recognition"
        );
        assert_eq!(papers[0].year, Some(2016));
        assert_eq!(papers[0].tags, vec!["resnet"]);
        // Dispatch must also see `itemType` under `data`, or the file
        // silently lands in the CSL parser and yields nothing.
        assert_eq!(parse_json_auto(api).unwrap()[0].year, Some(2016));
    }

    /// A type-mismatched item used to become an empty default item and be
    /// dropped with no signal, silently shrinking the import.
    #[test]
    fn malformed_zotero_item_fails_loudly() {
        let zot = r#"[
            {"itemType": "journalArticle", "title": "Good"},
            {"itemType": "journalArticle", "title": 42}
        ]"#;
        let err = parse_zotero_json(zot).unwrap_err();
        assert!(err.to_string().contains("1 item(s) failed"));
    }

    #[test]
    fn year_parsed_from_free_form_zotero_dates() {
        assert_eq!(year_from_date_string("2013-08-01"), Some(2013));
        assert_eq!(year_from_date_string("August 2013"), Some(2013));
        assert_eq!(year_from_date_string("2013"), Some(2013));
        assert_eq!(year_from_date_string("no date here"), None);
        // A longer digit run is an identifier, not a year.
        assert_eq!(year_from_date_string("123456"), None);
    }
}
