use async_trait::async_trait;
use serde::Deserialize;

use crate::adapters::semantic_scholar_source::percent_encode;
use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::paper_source::PaperSource;

/// Europe PMC (ebi.ac.uk) REST search — one keyless JSON endpoint covering
/// PubMed/MEDLINE records and, via the `SRC:PPR` filter, preprint servers
/// (bioRxiv, medRxiv, …). Both `EuropePmcSource` (biomedical literature) and
/// `PreprintSource` (bioRxiv-style preprints only) share the same fetch path,
/// modeled on `OpenAlexSource`: reqwest + serde DTOs, explicit non-2xx
/// checks, and offline tests over a canned JSON fixture.
pub struct EuropePmcSource {
    client: reqwest::Client,
}

impl EuropePmcSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for EuropePmcSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Same endpoint, restricted to the preprint source (`SRC:PPR`) so ingest can
/// target bioRxiv-style servers directly.
pub struct PreprintSource {
    inner: EuropePmcSource,
}

impl PreprintSource {
    pub fn new() -> Self {
        Self {
            inner: EuropePmcSource::new(),
        }
    }
}

impl Default for PreprintSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the search and map `core` results to `Paper`s. `resultType=core` is
/// what carries `abstractText`; the default `lite` would omit it.
async fn fetch(client: &reqwest::Client, query: &str, limit: usize) -> Result<Vec<Paper>> {
    let url = format!(
        "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}&format=json\
         &resultType=core&pageSize={limit}",
        percent_encode(query),
    );
    let resp = client
        .get(&url)
        .header("User-Agent", "research-agent/0.1")
        .send()
        .await
        .map_err(|e| ResearchError::Source(format!("Europe PMC request failed: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(ResearchError::Source(format!(
            "Europe PMC API returned HTTP {status}"
        )));
    }

    let parsed: EpmcResponse = resp
        .json()
        .await
        .map_err(|e| ResearchError::Source(format!("Europe PMC parse failed: {e}")))?;

    Ok(parsed
        .result_list
        .results
        .unwrap_or_default()
        .into_iter()
        .filter_map(hit_to_paper)
        .collect())
}

fn hit_to_paper(hit: EpmcHit) -> Option<Paper> {
    let title = hit.title.filter(|t| !t.is_empty())?;
    let mut paper = Paper::new(title);
    paper.year = hit.pub_year.and_then(|y| y.parse().ok());
    paper.doi = hit.doi;
    paper.venue = hit
        .journal_info
        .and_then(|ji| ji.journal)
        .and_then(|j| j.title);
    paper.abstract_text = hit.abstract_text.unwrap_or_default();
    paper.authors = hit
        .author_list
        .map(|al| al.author)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| a.full_name)
        .collect();
    paper.url = Some(format!(
        "https://europepmc.org/article/{}/{}",
        hit.source.unwrap_or_else(|| "MED".into()),
        hit.id,
    ));
    Some(paper)
}

#[derive(Deserialize)]
struct EpmcResponse {
    #[serde(rename = "resultList")]
    result_list: EpmcResultList,
}

#[derive(Deserialize)]
struct EpmcResultList {
    #[serde(default, rename = "result")]
    results: Option<Vec<EpmcHit>>,
}

#[derive(Deserialize)]
struct EpmcHit {
    id: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "pubYear")]
    pub_year: Option<String>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default, rename = "abstractText")]
    abstract_text: Option<String>,
    #[serde(default, rename = "journalInfo")]
    journal_info: Option<EpmcJournalInfo>,
    #[serde(default, rename = "authorList")]
    author_list: Option<EpmcAuthorList>,
}

#[derive(Deserialize)]
struct EpmcAuthorList {
    #[serde(default)]
    author: Vec<EpmcAuthor>,
}

#[derive(Deserialize)]
struct EpmcJournalInfo {
    #[serde(default)]
    journal: Option<EpmcJournal>,
}

#[derive(Deserialize)]
struct EpmcJournal {
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct EpmcAuthor {
    #[serde(default, rename = "fullName")]
    full_name: Option<String>,
}

#[async_trait]
impl PaperSource for EuropePmcSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        fetch(&self.client, query, limit).await
    }

    fn name(&self) -> &str {
        "europepmc"
    }
}

#[async_trait]
impl PaperSource for PreprintSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        fetch(&self.inner.client, &format!("({query}) AND SRC:PPR"), limit).await
    }

    fn name(&self) -> &str {
        "preprints"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "hitCount": 1,
        "resultList": {"result": [{
            "id": "37654512",
            "source": "MED",
            "pmid": "37654512",
            "doi": "10.1038/s41586-023-06600-8",
            "title": "Biomedical superintelligence",
            "pubYear": "2023",
            "abstractText": "Large language models in medicine.",
            "journalInfo": {"journal": {"title": "Nature"}},
            "authorList": {"author": [{"fullName": "J. Zou"}, {"fullName": "R. B. Altman"}]}
        }]}
    }"#;

    #[test]
    fn parses_epmc_hit() {
        let parsed: EpmcResponse = serde_json::from_str(FIXTURE).unwrap();
        let mut hits = parsed.result_list.results.unwrap().into_iter();
        let paper = hit_to_paper(hits.next().unwrap()).unwrap();
        assert_eq!(paper.title, "Biomedical superintelligence");
        assert_eq!(paper.year, Some(2023));
        assert_eq!(paper.doi.as_deref(), Some("10.1038/s41586-023-06600-8"));
        assert_eq!(paper.venue.as_deref(), Some("Nature"));
        assert_eq!(paper.authors, vec!["J. Zou", "R. B. Altman"]);
        assert!(!paper.abstract_text.is_empty());
        assert_eq!(
            paper.url.as_deref(),
            Some("https://europepmc.org/article/MED/37654512")
        );
    }

    #[test]
    fn hit_without_title_is_skipped() {
        let parsed: EpmcResponse =
            serde_json::from_str(r#"{"resultList": {"result": [{"id": "1", "source": "PPR"}]}}"#)
                .unwrap();
        let hit = parsed
            .result_list
            .results
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert!(hit_to_paper(hit).is_none());
    }

    #[test]
    fn empty_result_list_ok() {
        let parsed: EpmcResponse =
            serde_json::from_str(r#"{"resultList": {"result": []}}"#).unwrap();
        assert!(parsed.result_list.results.unwrap().is_empty());
    }

    #[test]
    fn source_names() {
        assert_eq!(EuropePmcSource::new().name(), "europepmc");
        assert_eq!(PreprintSource::new().name(), "preprints");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Live round-trip against the real Europe PMC API. Ignored by default:
    /// it needs network access. Run explicitly with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_search_returns_papers() {
        let src = EuropePmcSource::new();
        let papers = src.fetch_papers("CRISPR base editing", 3).await.unwrap();
        println!("got {} papers", papers.len());
        for p in &papers {
            println!("- {} ({:?})", p.title, p.year);
        }
        assert!(!papers.is_empty());
    }
}
