use async_trait::async_trait;
use serde::Deserialize;

use crate::adapters::semantic_scholar_source::percent_encode;
use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::paper_source::PaperSource;

/// OpenAlex (api.openalex.org) — 250M+ works with free, keyless metadata.
/// Modeled on `SemanticScholarSource`: reqwest + serde DTOs, explicit non-2xx
/// checks, and offline tests over a canned JSON fixture.
pub struct OpenAlexSource {
    client: reqwest::Client,
}

impl OpenAlexSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for OpenAlexSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Rebuild the abstract from OpenAlex's inverted index: word → list of
/// positions in the abstract. Positions may arrive unordered.
fn reconstruct_abstract(inverted: &std::collections::HashMap<String, Vec<usize>>) -> String {
    let mut words: Vec<(usize, &str)> = inverted
        .iter()
        .flat_map(|(word, positions)| positions.iter().map(move |&pos| (pos, word.as_str())))
        .collect();
    words.sort_unstable_by_key(|(pos, _)| *pos);
    words
        .into_iter()
        .map(|(_, word)| word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `https://doi.org/10.1234/x` → `10.1234/x`; pass through anything else.
fn normalize_doi(doi: &str) -> String {
    doi.strip_prefix("https://doi.org/")
        .unwrap_or(doi)
        .to_string()
}

/// `https://openalex.org/W2741809807` → `W2741809807`.
fn openalex_work_id(id: &str) -> Option<String> {
    id.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

#[derive(Deserialize)]
struct OaResponse {
    results: Option<Vec<OaWork>>,
}

#[derive(Deserialize)]
struct OaWork {
    id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    publication_year: Option<u32>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    primary_location: Option<OaLocation>,
    #[serde(default)]
    authorships: Vec<OaAuthorship>,
    #[serde(default)]
    abstract_inverted_index: Option<std::collections::HashMap<String, Vec<usize>>>,
}

#[derive(Deserialize)]
struct OaLocation {
    #[serde(default)]
    source: Option<OaSource>,
}

#[derive(Deserialize)]
struct OaSource {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct OaAuthorship {
    #[serde(default)]
    author: Option<OaAuthor>,
}

#[derive(Deserialize)]
struct OaAuthor {
    #[serde(default)]
    display_name: Option<String>,
}

#[async_trait]
impl PaperSource for OpenAlexSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let url = format!(
            "https://api.openalex.org/works?search={}&per-page={limit}",
            percent_encode(query),
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "research-agent/0.1")
            .send()
            .await
            .map_err(|e| ResearchError::Source(format!("OpenAlex request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ResearchError::Source(format!(
                "OpenAlex API returned HTTP {status}"
            )));
        }

        let parsed: OaResponse = resp
            .json()
            .await
            .map_err(|e| ResearchError::Source(format!("OpenAlex parse failed: {e}")))?;

        let mut papers = Vec::new();
        for work in parsed.results.unwrap_or_default() {
            let Some(title) = work.display_name.filter(|t| !t.is_empty()) else {
                continue;
            };
            let mut paper = Paper::new(title);
            paper.year = work.publication_year;
            paper.doi = work.doi.as_deref().map(normalize_doi);
            paper.venue = work
                .primary_location
                .as_ref()
                .and_then(|loc| loc.source.as_ref())
                .and_then(|src| src.display_name.clone());
            paper.openalex_id = work.id.as_deref().and_then(openalex_work_id);
            paper.abstract_text = work
                .abstract_inverted_index
                .as_ref()
                .map(reconstruct_abstract)
                .unwrap_or_default();
            paper.authors = work
                .authorships
                .iter()
                .filter_map(|a| a.author.as_ref().and_then(|au| au.display_name.clone()))
                .collect();
            paper.url = work.id.clone();
            papers.push(paper);
        }
        Ok(papers)
    }

    fn name(&self) -> &str {
        "openalex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "meta": {"count": 1},
        "results": [{
            "id": "https://openalex.org/W2741809807",
            "doi": "https://doi.org/10.1038/nature12373",
            "display_name": "Nanometre-scale thermometry in a living cell",
            "publication_year": 2013,
            "primary_location": {
                "source": {"display_name": "Nature"}
            },
            "authorships": [
                {"author": {"display_name": "G. Kucsko"}},
                {"author": {"display_name": "P. C. Maurer"}}
            ],
            "abstract_inverted_index": {
                "Heat": [0], "is": [1], "generated": [2], "by": [3], "the": [4, 9],
                "cell": [5, 10]
            }
        }]
    }"#;

    #[test]
    fn parses_openalex_work() {
        let parsed: OaResponse = serde_json::from_str(FIXTURE).unwrap();
        let works = parsed.results.unwrap();
        assert_eq!(works.len(), 1);
        let work = works.into_iter().next().unwrap();
        assert_eq!(
            openalex_work_id(work.id.as_deref().unwrap()).unwrap(),
            "W2741809807"
        );
        assert_eq!(
            work.doi.as_deref().map(normalize_doi).unwrap(),
            "10.1038/nature12373"
        );
        assert_eq!(work.publication_year, Some(2013));
    }

    #[test]
    fn reconstructs_abstract_from_inverted_index() {
        let parsed: OaResponse = serde_json::from_str(FIXTURE).unwrap();
        let work = &parsed.results.unwrap()[0];
        let inverted = work.abstract_inverted_index.as_ref().unwrap();
        let abstract_text = reconstruct_abstract(inverted);
        assert_eq!(abstract_text, "Heat is generated by the cell the cell");
    }

    #[test]
    fn missing_fields_tolerated() {
        let parsed: OaResponse =
            serde_json::from_str(r#"{"results": [{"id": "https://openalex.org/W1"}]}"#).unwrap();
        let work = &parsed.results.unwrap()[0];
        assert!(work.display_name.is_none());
        assert!(work.abstract_inverted_index.is_none());
        assert!(work.authorships.is_empty());
    }

    #[test]
    fn source_name() {
        assert_eq!(OpenAlexSource::new().name(), "openalex");
    }
}
