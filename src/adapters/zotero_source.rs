//! Read papers from a running Zotero instance over its local HTTP API.
//!
//! One-way read as another ingest source, per the roadmap verdict: reads need
//! no auth, while writes drag in single-use keys and conflict resolution with
//! no merge UI. Requires Zotero running with the "Allow other applications on
//! this computer to communicate with Zotero" preference enabled — every
//! request 403s without it.
//!
//! Docs: <https://www.zotero.org/support/dev/web_api/v3/local_api>

use async_trait::async_trait;
use serde::Deserialize;

use crate::adapters::bib_importer::{ZoteroItem, papers_from_zotero_items};
use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::paper_source::PaperSource;

/// Default local-API base. `ZOTERO_BASE_URL` overrides it (non-default
/// installs, tests).
const DEFAULT_BASE_URL: &str = "http://localhost:23119/api/";

pub struct ZoteroSource {
    base_url: String,
    client: reqwest::Client,
}

impl ZoteroSource {
    pub fn new() -> Self {
        let base_url =
            std::env::var("ZOTERO_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Self::with_base_url(base_url)
    }

    pub fn with_base_url(base_url: String) -> Self {
        // The endpoint is appended without a leading slash, so a base the user
        // wrote without a trailing slash still produces the right URL.
        let base_url = if base_url.ends_with('/') {
            base_url
        } else {
            format!("{base_url}/")
        };
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ZoteroSource {
    fn default() -> Self {
        Self::new()
    }
}

/// One entry of a local-API items response: Zotero item JSON nested under
/// `data`, plus bookkeeping fields the export format does not carry.
#[derive(Deserialize)]
struct ApiItem {
    data: ZoteroItem,
}

/// Zotero caps a single page at 100 items, so `--limit 500` is served as five
/// paged requests, not one silently truncated one.
const PAGE_SIZE: usize = 100;

/// Query parameters for one page of the items endpoint. An empty query omits
/// `q` (the whole library); `qmode=everything` searches all fields, not just
/// the title-creator-year subset the server defaults to.
fn items_params(query: &str, start: usize, limit: usize) -> Vec<(String, String)> {
    let mut params = vec![
        ("format".to_string(), "json".to_string()),
        ("qmode".to_string(), "everything".to_string()),
        ("start".to_string(), start.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if !query.trim().is_empty() {
        params.push(("q".to_string(), query.to_string()));
    }
    params
}

/// Turn an HTTP status into an error naming the fix, so a 403 does not read
/// like a bug.
fn status_error(status: reqwest::StatusCode) -> ResearchError {
    if status == reqwest::StatusCode::FORBIDDEN {
        return ResearchError::Source(
            "Zotero returned HTTP 403 — enable \"Allow other applications on \
             this computer to communicate with Zotero\" in Zotero settings"
                .to_string(),
        );
    }
    ResearchError::Source(format!("Zotero local API returned HTTP {status}"))
}

#[async_trait]
impl PaperSource for ZoteroSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let mut papers = Vec::new();
        let mut start = 0;
        while papers.len() < limit {
            let page_limit = PAGE_SIZE.min(limit - papers.len());
            let resp = self
                .client
                .get(format!("{}users/0/items", self.base_url))
                .query(&items_params(query, start, page_limit))
                .send()
                .await
                .map_err(|e| {
                    if e.is_connect() {
                        ResearchError::Source(format!(
                            "could not reach Zotero at {} (is Zotero running?): {e}",
                            self.base_url
                        ))
                    } else {
                        ResearchError::Source(format!("Zotero request failed: {e}"))
                    }
                })?;

            let status = resp.status();
            if !status.is_success() {
                return Err(status_error(status));
            }

            let body = resp
                .text()
                .await
                .map_err(|e| ResearchError::Source(format!("Zotero response read failed: {e}")))?;
            let items: Vec<ApiItem> = serde_json::from_str(&body).map_err(|e| {
                ResearchError::Source(format!("Zotero local API response parse failed: {e}"))
            })?;

            let got = items.len();
            papers.extend(papers_from_zotero_items(
                items.into_iter().map(|i| i.data).collect(),
            ));
            // A short page means the library is exhausted; an empty page
            // guards the loop when Zotero reports more than it returns.
            if got < page_limit {
                break;
            }
            start += got;
        }
        papers.truncate(limit);
        Ok(papers)
    }

    fn name(&self) -> &str {
        "zotero"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local-API shape: item JSON nested under `data`. The mapping itself is
    /// exercised in `bib_importer`; here only the nesting and filtering are
    /// under test.
    const API_FIXTURE: &str = r#"[
        {
            "key": "ABCD1234",
            "version": 3,
            "library": {"type": "user", "id": 0},
            "data": {
                "itemType": "journalArticle",
                "title": "Thermometry in a living cell",
                "abstractNote": "We report nanoscale thermometry.",
                "publicationTitle": "Nature",
                "DOI": "https://doi.org/10.1038/NATURE12373",
                "date": "August 2013",
                "creators": [
                    {"firstName": "Georg", "lastName": "Kucsko"},
                    {"name": "Some Institute"}
                ],
                "tags": [{"tag": "quantum sensing"}]
            }
        },
        {
            "key": "ATTACH01",
            "data": {"itemType": "attachment", "title": "Full Text PDF"}
        }
    ]"#;

    fn parse_fixture(content: &str) -> Vec<Paper> {
        let items: Vec<ApiItem> = serde_json::from_str(content).unwrap();
        papers_from_zotero_items(items.into_iter().map(|i| i.data).collect())
    }

    #[test]
    fn parses_nested_api_items_and_drops_attachments() {
        let papers = parse_fixture(API_FIXTURE);
        assert_eq!(papers.len(), 1);
        let paper = &papers[0];
        assert_eq!(paper.title, "Thermometry in a living cell");
        assert_eq!(paper.abstract_text, "We report nanoscale thermometry.");
        assert_eq!(paper.authors, vec!["Georg Kucsko", "Some Institute"]);
        assert_eq!(paper.year, Some(2013));
        assert_eq!(paper.venue.as_deref(), Some("Nature"));
        // Normalized on the way in, so it dedupes against export-file imports.
        assert_eq!(paper.doi.as_deref(), Some("10.1038/nature12373"));
        assert_eq!(paper.tags, vec!["quantum sensing"]);
    }

    #[test]
    fn empty_api_response_yields_no_papers() {
        let papers = parse_fixture("[]");
        assert!(papers.is_empty());
    }

    #[test]
    fn items_params_search_all_fields() {
        let params = items_params("deep & learning #1", 0, 10);
        assert!(params.contains(&("qmode".to_string(), "everything".to_string())));
        assert!(params.contains(&("q".to_string(), "deep & learning #1".to_string())));
        assert!(params.contains(&("start".to_string(), "0".to_string())));
        assert!(params.contains(&("limit".to_string(), "10".to_string())));
    }

    #[test]
    fn items_params_empty_query_omits_q() {
        let params = items_params("  ", 100, 50);
        assert!(!params.iter().any(|(k, _)| k == "q"));
        assert!(params.contains(&("start".to_string(), "100".to_string())));
        assert!(params.contains(&("limit".to_string(), "50".to_string())));
    }

    #[test]
    fn base_url_gets_trailing_slash() {
        let src = ZoteroSource::with_base_url("http://localhost:9999/api".to_string());
        assert_eq!(src.base_url, "http://localhost:9999/api/");
    }

    #[test]
    fn forbidden_status_names_the_preference() {
        let err = status_error(reqwest::StatusCode::FORBIDDEN).to_string();
        assert!(err.contains("Allow other applications"), "got: {err}");
    }

    /// A refused connection (nothing on 127.0.0.1:1) must produce the "is
    /// Zotero running?" message, not a bare reqwest dump. Hermetic: localhost
    /// only, no external network.
    #[tokio::test]
    async fn refused_connection_says_zotero_may_not_be_running() {
        let source = ZoteroSource::with_base_url("http://127.0.0.1:1/api/".to_string());
        let err = source.fetch_papers("", 1).await.unwrap_err().to_string();
        assert!(err.contains("is Zotero running?"), "got: {err}");
    }

    /// Live round-trip against a running Zotero. Ignored by default: it needs
    /// Zotero up with the local-API preference enabled. Run explicitly with
    /// `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "hits the live Zotero local API (needs Zotero running)"]
    async fn fetches_live_zotero_library() {
        let source = ZoteroSource::new();
        let papers = source.fetch_papers("", 5).await.unwrap();
        assert!(
            !papers.is_empty(),
            "a running Zotero with items returns them"
        );
    }
}
