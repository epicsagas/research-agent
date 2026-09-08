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

/// One resolved citation intent: which paper the edge points at, plus the
/// Semantic Scholar labels for it.
#[derive(Debug, Clone)]
pub struct CitationIntent {
    /// DOI of the other paper on the edge, when S2 knows one.
    pub doi: Option<String>,
    /// Semantic Scholar paper id of the other paper.
    pub s2_id: Option<String>,
    /// Intent labels, e.g. `background`, `methodology`, `result`. Empty when
    /// S2 has not classified the edge, which is common.
    pub intents: Vec<String>,
    /// S2's "influential citation" flag for the edge.
    pub influential: bool,
}

impl CitationIntent {
    /// Compact label for the `citations.context` column: intents joined by
    /// `+`, with a `influential` marker appended. Empty when S2 classified
    /// nothing, so unlabeled edges stay indistinguishable from never-synced
    /// ones by design (both mean "no evidence").
    pub fn label(&self) -> String {
        let mut parts = self.intents.clone();
        if self.influential {
            parts.push("influential".to_string());
        }
        parts.join("+")
    }
}

#[derive(Deserialize)]
struct S2CitationsResponse {
    data: Option<Vec<S2CitationEdge>>,
}

#[derive(Deserialize)]
struct S2CitationEdge {
    #[serde(default)]
    intents: Option<Vec<String>>,
    #[serde(rename = "isInfluential", default)]
    is_influential: Option<bool>,
    /// Present on `/references` responses.
    #[serde(rename = "citedPaper", default)]
    cited_paper: Option<S2Paper>,
    /// Present on `/citations` responses.
    #[serde(rename = "citingPaper", default)]
    citing_paper: Option<S2Paper>,
}

impl S2CitationEdge {
    fn into_intent(self) -> Option<CitationIntent> {
        let other = self.cited_paper.or(self.citing_paper)?;
        Some(CitationIntent {
            doi: other.external_ids.as_ref().and_then(|e| e.doi.clone()),
            s2_id: other.paper_id,
            intents: self.intents.unwrap_or_default(),
            influential: self.is_influential.unwrap_or(false),
        })
    }
}

impl SemanticScholarSource {
    /// Fetch per-edge citation intents for one paper. `direction` picks the
    /// S2 endpoint: `references` (works this paper cites) or `citations`
    /// (works citing it). The paper is addressed by DOI or S2 id; anything
    /// else cannot be resolved.
    ///
    /// Intents are sparse upstream: S2 classifies only a fraction of edges
    /// (roughly 35-80% in sampling), so an empty `intents` list is a normal
    /// result, not an error.
    // ponytail: first page only (S2 caps limit at 1000); paginate via `next`
    // when papers with more edges than that need full coverage.
    pub async fn citation_intents(
        &self,
        paper: &Paper,
        direction: &str,
    ) -> Result<Vec<CitationIntent>> {
        let key = match (&paper.s2_id, &paper.doi) {
            (Some(id), _) => id.clone(),
            (None, Some(doi)) => format!("DOI:{doi}"),
            (None, None) => {
                return Err(ResearchError::Source(
                    "paper has no Semantic Scholar id or DOI; cannot resolve citation intents"
                        .into(),
                ));
            }
        };
        let endpoint = match direction {
            "references" | "citations" => direction,
            other => {
                return Err(ResearchError::Source(format!(
                    "unknown citation direction '{other}'"
                )));
            }
        };
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}/{endpoint}?fields=intents,isInfluential,externalIds&limit=1000",
            percent_encode(&key),
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "research-agent/0.1")
            .send()
            .await
            .map_err(|e| ResearchError::Source(format!("S2 request failed: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            // 429 is the common failure here: the keyless tier is heavily
            // rate limited, so name it rather than leaving a bare status.
            if status.as_u16() == 429 {
                return Err(ResearchError::Source(
                    "S2 API rate limit hit (HTTP 429); retry later or configure an API key".into(),
                ));
            }
            return Err(ResearchError::Source(format!(
                "S2 API returned HTTP {status}"
            )));
        }
        let parsed: S2CitationsResponse = resp
            .json()
            .await
            .map_err(|e| ResearchError::Source(format!("S2 JSON parse failed: {e}")))?;
        Ok(parsed
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| e.into_intent())
            .collect())
    }
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

    /// Both endpoints share one DTO: `/references` nests `citedPaper`,
    /// `/citations` nests `citingPaper`.
    #[test]
    fn parses_both_citation_edge_shapes() {
        let refs = r#"{"data":[{"isInfluential":true,"intents":["methodology"],"citedPaper":{"paperId":"p1","externalIds":{"DOI":"10.1/a"}}}]}"#;
        let parsed: S2CitationsResponse = serde_json::from_str(refs).unwrap();
        let edge = parsed.data.unwrap().pop().unwrap().into_intent().unwrap();
        assert_eq!(edge.doi.as_deref(), Some("10.1/a"));
        assert_eq!(edge.s2_id.as_deref(), Some("p1"));
        assert_eq!(edge.label(), "methodology+influential");

        let cites = r#"{"data":[{"isInfluential":false,"intents":[],"citingPaper":{"paperId":"p2","externalIds":{"DOI":"10.1/b"}}}]}"#;
        let parsed: S2CitationsResponse = serde_json::from_str(cites).unwrap();
        let edge = parsed.data.unwrap().pop().unwrap().into_intent().unwrap();
        assert_eq!(edge.doi.as_deref(), Some("10.1/b"));
        // Unclassified edges are the common upstream case: no label, no marker.
        assert_eq!(edge.label(), "");
    }

    /// S2 returns edges whose other paper it cannot resolve; those carry no
    /// nested paper object and must be dropped, not panic.
    #[test]
    fn edge_without_paper_is_skipped() {
        let json = r#"{"data":[{"isInfluential":false,"intents":["background"]}]}"#;
        let parsed: S2CitationsResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.data.unwrap().pop().unwrap().into_intent().is_none());
    }

    #[tokio::test]
    async fn citation_intents_rejects_unknown_direction() {
        let mut paper = Paper::new("x".into());
        paper.doi = Some("10.1/a".into());
        let err = SemanticScholarSource::new()
            .citation_intents(&paper, "sideways")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown citation direction"));
    }

    #[tokio::test]
    async fn citation_intents_requires_identity() {
        let paper = Paper::new("no ids".into());
        let err = SemanticScholarSource::new()
            .citation_intents(&paper, "references")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no Semantic Scholar id or DOI"));
    }
}
