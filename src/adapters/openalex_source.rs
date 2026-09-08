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
    #[serde(default)]
    referenced_works: Vec<String>,
}

/// Shared mapping for both the search and reference endpoints.
fn work_to_paper(work: OaWork) -> Option<Paper> {
    let title = work.display_name.filter(|t| !t.is_empty())?;
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
    Some(paper)
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

        Ok(parsed
            .results
            .unwrap_or_default()
            .into_iter()
            .filter_map(work_to_paper)
            .collect())
    }

    fn name(&self) -> &str {
        "openalex"
    }
}

/// Citation-graph half of the OpenAlex adapter: resolve the `W…` ids a paper
/// references, then hydrate those ids into `Paper` records; and the reverse,
/// list the works citing a paper via the `cites:` filter.
pub struct ReferencesSource {
    client: reqwest::Client,
}

impl ReferencesSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// OpenAlex work ids (`W…`) referenced by `paper`. The paper must carry an
    /// `openalex_id` or a DOI; anything else cannot be resolved.
    pub async fn reference_ids(&self, paper: &Paper) -> Result<Vec<String>> {
        let key = match (&paper.openalex_id, &paper.doi) {
            (Some(id), _) => id.clone(),
            (None, Some(doi)) => format!("doi:{doi}"),
            (None, None) => {
                return Err(ResearchError::Source(
                    "paper has no openalex_id or DOI; cannot resolve references".into(),
                ));
            }
        };
        let work = self.fetch_work(&key).await?;
        Ok(work
            .referenced_works
            .iter()
            .filter_map(|u| openalex_work_id(u))
            .collect())
    }

    /// Works citing `paper`, hydrated. The `cites:` filter needs a `W…` id, so
    /// a DOI-only paper is resolved through one extra lookup.
    // ponytail: first page only (200 works, the OpenAlex per-page max);
    // cursor-pagination the `meta.next_cursor` loop when full citing sets matter.
    pub async fn citing_papers(&self, paper: &Paper) -> Result<Vec<Paper>> {
        let work_id = match &paper.openalex_id {
            Some(id) => id.clone(),
            None => {
                let doi = paper.doi.as_ref().ok_or_else(|| {
                    ResearchError::Source(
                        "paper has no openalex_id or DOI; cannot resolve citers".into(),
                    )
                })?;
                let work = self.fetch_work(&format!("doi:{doi}")).await?;
                openalex_work_id(work.id.as_deref().unwrap_or_default()).ok_or_else(|| {
                    ResearchError::Source("OpenAlex returned a work without an id".into())
                })?
            }
        };
        let works = self
            .fetch_results(&format!(
                "https://api.openalex.org/works?filter=cites:{work_id}&per-page=200"
            ))
            .await?;
        Ok(works.into_iter().filter_map(work_to_paper).collect())
    }

    /// Hydrate work ids into papers. One batched request per chunk (the
    /// `openalex_id:` filter accepts `|`-joined ids; per-page caps at 200).
    pub async fn hydrate(&self, work_ids: &[String]) -> Result<Vec<Paper>> {
        let mut papers = Vec::new();
        for chunk in work_ids.chunks(50) {
            let url = format!(
                "https://api.openalex.org/works?filter=openalex_id:{}&per-page={}",
                chunk.join("|"),
                chunk.len(),
            );
            let works = self.fetch_results(&url).await?;
            papers.extend(works.into_iter().filter_map(work_to_paper));
        }
        Ok(papers)
    }

    /// GET a works-list URL and unwrap its `results`.
    async fn fetch_results(&self, url: &str) -> Result<Vec<OaWork>> {
        let resp = self
            .client
            .get(url)
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
        Ok(parsed.results.unwrap_or_default())
    }

    async fn fetch_work(&self, key: &str) -> Result<OaWork> {
        let url = format!("https://api.openalex.org/works/{}", percent_encode(key));
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
        resp.json()
            .await
            .map_err(|e| ResearchError::Source(format!("OpenAlex parse failed: {e}")))
    }
}

impl Default for ReferencesSource {
    fn default() -> Self {
        Self::new()
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

    const REFS_FIXTURE: &str = r#"{
        "id": "https://openalex.org/W2741809807",
        "display_name": "Nanometre-scale thermometry in a living cell",
        "referenced_works": [
            "https://openalex.org/W1560783210",
            "https://openalex.org/W1724212071"
        ]
    }"#;

    #[test]
    fn extracts_reference_ids_from_urls() {
        let work: OaWork = serde_json::from_str(REFS_FIXTURE).unwrap();
        let ids: Vec<String> = work
            .referenced_works
            .iter()
            .filter_map(|u| openalex_work_id(u))
            .collect();
        assert_eq!(ids, vec!["W1560783210", "W1724212071"]);
    }

    #[test]
    fn hydrates_referenced_works() {
        let parsed: OaResponse = serde_json::from_str(
            r#"{"results": [{"id": "https://openalex.org/W1560783210", "display_name": "Anatomy of green open access"}]}"#,
        )
        .unwrap();
        let papers: Vec<Paper> = parsed
            .results
            .unwrap()
            .into_iter()
            .filter_map(work_to_paper)
            .collect();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].openalex_id.as_deref(), Some("W1560783210"));
        assert_eq!(papers[0].title, "Anatomy of green open access");
    }
}
