use async_trait::async_trait;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::domain::paper::Paper;
use crate::error::{ResearchError, Result};
use crate::ports::paper_source::PaperSource;

pub struct ArxivSource {
    client: reqwest::Client,
}

impl ArxivSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ArxivSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PaperSource for ArxivSource {
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let encoded = query.replace(' ', "+");
        let url = format!(
            "https://export.arxiv.org/api/query?search_query=all:{}&max_results={}",
            encoded, limit
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ResearchError::Source(format!("arXiv request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ResearchError::Source(format!(
                "arXiv API returned HTTP {status}"
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ResearchError::Source(format!("arXiv response read failed: {e}")))?;

        parse_arxiv_atom(&body)
    }

    fn name(&self) -> &str {
        "arxiv"
    }
}

/// Resolve an XML entity reference (`"amp"`, `"#39"`, `"#x27"`, …) to its
/// text. Only the five predefined entities and numeric character references
/// can occur in an arXiv Atom document — it carries no DTD. Unknown references
/// are dropped rather than failing the whole ingest.
fn resolve_ref(name: &str) -> Option<String> {
    match name {
        "amp" => Some("&".into()),
        "lt" => Some("<".into()),
        "gt" => Some(">".into()),
        "quot" => Some("\"".into()),
        "apos" => Some("'".into()),
        other => other
            .strip_prefix('#')
            .and_then(|num| {
                num.strip_prefix('x')
                    .or_else(|| num.strip_prefix('X'))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| num.parse::<u32>().ok())
            })
            .and_then(char::from_u32)
            .map(|c| c.to_string()),
    }
}

fn parse_arxiv_atom(xml: &str) -> Result<Vec<Paper>> {
    // NB: `trim_text` stays off — it trims each text *event*, and an entity
    // reference splits an element's text into several events, so interior
    // whitespace would be lost ("R&D 'x'" -> "R&D'x'"). Accumulators are
    // trimmed where they are consumed instead.
    let mut reader = Reader::from_str(xml);

    let mut papers: Vec<Paper> = Vec::new();

    // Per-entry accumulators
    let mut in_entry = false;
    let mut current_tag = String::new();
    let mut title = String::new();
    let mut summary = String::new();
    let mut arxiv_id = String::new();
    let mut published = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut author_name = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                // Strip namespace prefix (e.g. "atom:entry" → "entry")
                let local = e.name().local_name().into_inner();
                current_tag = local.to_string();
                if local == "entry" {
                    in_entry = true;
                    title.clear();
                    summary.clear();
                    arxiv_id.clear();
                    published.clear();
                    authors.clear();
                }
                // Text and entity references both append, so start each
                // element's accumulator fresh.
                match local {
                    "title" => title.clear(),
                    "summary" => summary.clear(),
                    "id" => arxiv_id.clear(),
                    "published" => published.clear(),
                    "name" => author_name.clear(),
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.name().local_name().into_inner();
                current_tag.clear();
                if local == "name" && in_entry && !author_name.is_empty() {
                    authors.push(author_name.trim().to_string());
                    author_name.clear();
                }
                if local == "entry" && in_entry {
                    let mut paper = Paper::new(title.trim().to_string());
                    paper.abstract_text = summary.trim().to_string();
                    paper.authors = authors.clone();
                    paper.year = published
                        .trim()
                        .get(..4)
                        .and_then(|y| y.parse::<u32>().ok());

                    // arxiv_id from <id>http://arxiv.org/abs/2301.00234v1</id>
                    let last = arxiv_id.split('/').next_back().unwrap_or("");
                    // strip version suffix "v<digits>" if present
                    let bare_id = if let Some(pos) = last.rfind('v') {
                        let after_v = &last[pos + 1..];
                        if after_v.chars().all(|c| c.is_ascii_digit()) && !after_v.is_empty() {
                            last[..pos].to_string()
                        } else {
                            last.to_string()
                        }
                    } else {
                        last.to_string()
                    };
                    if !bare_id.is_empty() {
                        paper.url = Some(format!("https://arxiv.org/abs/{bare_id}"));
                        paper.arxiv_id = Some(bare_id);
                    }

                    papers.push(paper);
                    in_entry = false;
                    current_tag.clear();
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.xml10_content().to_string();
                if in_entry {
                    match current_tag.as_str() {
                        "title" => title.push_str(&text),
                        "summary" => summary.push_str(&text),
                        "id" => arxiv_id.push_str(&text),
                        "published" => published.push_str(&text),
                        "name" => author_name.push_str(&text),
                        _ => {}
                    }
                }
            }
            // An `&entity;` inside element text arrives as its own event; the
            // surrounding Text pieces are delivered separately.
            Ok(Event::GeneralRef(e)) if in_entry => {
                let text = resolve_ref(e.xml10_content().as_ref()).unwrap_or_default();
                match current_tag.as_str() {
                    "title" => title.push_str(&text),
                    "summary" => summary.push_str(&text),
                    "id" => arxiv_id.push_str(&text),
                    "published" => published.push_str(&text),
                    "name" => author_name.push_str(&text),
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ResearchError::Source(format!("arXiv XML parse error: {e}"))),
            _ => {}
        }
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATOM_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.00234v1</id>
    <title>Async Runtimes in Rust</title>
    <summary>A survey of async runtime design.</summary>
    <published>2023-01-15T00:00:00Z</published>
    <author><name>Alice Smith</name></author>
    <author><name>Bob Jones</name></author>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/2302.00100v2</id>
    <title>Tokio Internals</title>
    <summary>Deep dive into Tokio scheduling.</summary>
    <published>2023-02-01T00:00:00Z</published>
    <author><name>Carol White</name></author>
  </entry>
</feed>"#;

    #[test]
    fn parse_atom_returns_papers() {
        let papers = parse_arxiv_atom(ATOM_FIXTURE).unwrap();
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].title, "Async Runtimes in Rust");
        assert_eq!(papers[0].authors, vec!["Alice Smith", "Bob Jones"]);
        assert_eq!(papers[0].year, Some(2023));
        assert_eq!(papers[0].arxiv_id.as_deref(), Some("2301.00234"));
        assert_eq!(papers[1].title, "Tokio Internals");
    }

    #[test]
    fn parse_atom_empty_feed() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        let papers = parse_arxiv_atom(xml).unwrap();
        assert!(papers.is_empty());
    }

    /// Entity references in text must survive the quick-xml 0.42 event model,
    /// where `&…;` is delivered as its own `GeneralRef` event.
    #[test]
    fn parse_atom_unescapes_entities() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2303.00001v1</id>
    <title>Q&amp;A for R&#38;D &#x27;scaling&#x27;</title>
    <summary>AT&amp;T results &lt;published&gt; here.</summary>
    <author><name>A &amp; B</name></author>
  </entry>
</feed>"#;
        let papers = parse_arxiv_atom(xml).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].title, "Q&A for R&D 'scaling'");
        assert_eq!(papers[0].abstract_text, "AT&T results <published> here.");
        assert_eq!(papers[0].authors, vec!["A & B"]);
    }

    #[test]
    fn source_name() {
        let source = ArxivSource::new();
        assert_eq!(source.name(), "arxiv");
    }
}
