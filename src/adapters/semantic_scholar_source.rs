use async_trait::async_trait;
use serde::Deserialize;

use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::paper_source::PaperSource;

pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(
                    char::from_digit((b >> 4) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
                out.push(
                    char::from_digit((b & 0xf) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    out
}

pub struct SemanticScholarSource {
    client: reqwest::Client,
}

impl SemanticScholarSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for SemanticScholarSource {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct S2Response {
    data: Option<Vec<S2Paper>>,
}

#[derive(Deserialize)]
struct S2Paper {
    #[serde(rename = "paperId")]
    paper_id: Option<String>,
    title: Option<String>,
    authors: Option<Vec<S2Author>>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    year: Option<u32>,
    venue: Option<String>,
    #[serde(rename = "externalIds")]
    external_ids: Option<S2ExternalIds>,
}

#[derive(Deserialize)]
struct S2Author {
    name: Option<String>,
}

#[derive(Deserialize)]
struct S2ExternalIds {
    #[serde(rename = "ArXiv")]
    arxiv: Option<String>,
    #[serde(rename = "DOI")]
    doi: Option<String>,
}

#[async_trait]
impl PaperSource for SemanticScholarSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let encoded = percent_encode(query);
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields=title,authors,abstract,year,venue,externalIds",
            encoded, limit
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "research-agent/0.1")
            .send()
            .await
            .map_err(|e| ResearchError::Source(format!("S2 request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ResearchError::Source(format!(
                "S2 API returned HTTP {status}"
            )));
        }

        let s2: S2Response = resp
            .json()
            .await
            .map_err(|e| ResearchError::Source(format!("S2 JSON parse failed: {e}")))?;

        let papers = s2
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let mut paper = Paper::new(s.title.unwrap_or_default());
                paper.s2_id = s.paper_id;
                paper.abstract_text = s.abstract_text.unwrap_or_default();
                paper.authors = s
                    .authors
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|a| a.name)
                    .collect();
                paper.year = s.year;
                paper.venue = s.venue;
                if let Some(ext) = s.external_ids {
                    paper.arxiv_id = ext.arxiv;
                    paper.doi = ext.doi;
                }
                paper
            })
            .collect();

        Ok(papers)
    }

    fn name(&self) -> &str {
        "semantic_scholar"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_s2_json() {
        let json = r#"{"data":[{"paperId":"abc123","title":"Attention Is All You Need","authors":[{"name":"Vaswani"}],"abstract":"Transformer architecture.","year":2017,"venue":"NeurIPS","externalIds":{"ArXiv":"1706.03762","DOI":"10.1145/123"}}]}"#;
        let s2: S2Response = serde_json::from_str(json).unwrap();
        let items = s2.data.unwrap_or_default();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Attention Is All You Need"));
        assert_eq!(items[0].paper_id.as_deref(), Some("abc123"));
        assert_eq!(items[0].year, Some(2017));
        assert_eq!(
            items[0]
                .external_ids
                .as_ref()
                .and_then(|e| e.arxiv.as_deref()),
            Some("1706.03762")
        );
    }

    #[test]
    fn parse_s2_empty() {
        let json = r#"{"data":[]}"#;
        let s2: S2Response = serde_json::from_str(json).unwrap();
        assert!(s2.data.unwrap_or_default().is_empty());
    }

    #[test]
    fn parse_s2_null_data() {
        let json = r#"{"total":0}"#;
        let s2: S2Response = serde_json::from_str(json).unwrap();
        assert!(s2.data.unwrap_or_default().is_empty());
    }

    #[test]
    fn source_name() {
        let source = SemanticScholarSource::new();
        assert_eq!(source.name(), "semantic_scholar");
    }
}
