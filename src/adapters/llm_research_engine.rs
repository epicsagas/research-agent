use async_trait::async_trait;
use llm_kernel::llm::{
    AnthropicClient, ChatMessage, LLMClient, LLMRequest, ModelConfig, OpenAIClient,
};
use llm_kernel::safety::sanitize_output;
use llm_kernel::tokens::estimate_tokens;

use crate::domain::knowledge_gap::{GapType, KnowledgeGap};
use crate::domain::research_report::{ReportSection, ResearchReport};
use crate::error::{ResearchError, Result};
use crate::ports::index_store::IndexStore;
use crate::ports::research_engine::ResearchEngine;

pub struct LlmResearchEngine {
    store: Box<dyn IndexStore>,
    config: Option<ModelConfig>,
}

impl LlmResearchEngine {
    pub fn new(store: Box<dyn IndexStore>) -> Self {
        Self {
            store,
            config: None,
        }
    }

    pub fn with_config(store: Box<dyn IndexStore>, config: ModelConfig) -> Self {
        Self {
            store,
            config: Some(config),
        }
    }

    fn make_client(config: &ModelConfig) -> Result<Box<dyn LLMClient>> {
        if config.provider == "anthropic" {
            AnthropicClient::new(config)
                .map(|c| Box::new(c) as Box<dyn LLMClient>)
                .map_err(|e| ResearchError::Source(e.to_string()))
        } else {
            OpenAIClient::new(config)
                .map(|c| Box::new(c) as Box<dyn LLMClient>)
                .map_err(|e| ResearchError::Source(e.to_string()))
        }
    }

    /// Build the gap-analysis context (paper abstracts) for a single topic.
    /// Returns only papers explicitly linked to the topic via `topic_papers`.
    /// When the topic has no linked papers, returns a clearly-marked placeholder
    /// so the LLM never receives arbitrary papers from unrelated topics.
    fn gap_context_for_topic(&self, topic_id: &str) -> Result<String> {
        // No count limit: the store already orders linked papers by relevance
        // DESC, so the token cap below — not an arbitrary count — decides how
        // much context the LLM sees.
        let papers = self.store.list_papers_by_topic(topic_id, None)?;
        if papers.is_empty() {
            return Ok("(no papers indexed for this topic yet)".into());
        }
        let abstracts: String = papers
            .iter()
            .map(|p| format!("Title: {}\nAbstract: {}\n", p.title, p.abstract_text))
            .collect::<Vec<_>>()
            .join("\n---\n");
        Ok(if estimate_tokens(&abstracts) > 6000 {
            abstracts.chars().take(24000).collect()
        } else {
            abstracts
        })
    }

    /// Build the per-topic paper block for report context. Returns only papers
    /// linked to the topic; a placeholder is emitted when none are linked, so
    /// the report never describes arbitrary papers as belonging to the topic.
    fn report_context_for_topic(&self, topic_id: &str) -> Result<String> {
        // No count limit: papers arrive relevance-ordered and the token
        // budget below decides how many fit — a fixed count silently drops
        // linked papers (and with equal scores, arbitrary ones).
        let papers = self.store.list_papers_by_topic(topic_id, None)?;
        if papers.is_empty() {
            return Ok("  (no papers indexed for this topic yet)\n".into());
        }
        let mut out = String::new();
        let mut budget = REPORT_CONTEXT_TOKENS;
        for (i, paper) in papers.iter().enumerate() {
            let line = format!("- {}: {}\n", paper.title, paper.abstract_text);
            let cost = estimate_tokens(&line);
            // Papers arrive relevance-ordered; the first one that no longer
            // fits ends the block (the first paper is always included).
            if i > 0 && cost > budget {
                break;
            }
            budget = budget.saturating_sub(cost);
            out.push_str(&line);
        }
        Ok(out)
    }
}

/// Token budget for report context: include as many linked papers (highest
/// relevance first) as fit instead of a fixed count.
const REPORT_CONTEXT_TOKENS: usize = 4000;

/// Parse `GAP_TYPE|description` lines out of an LLM response. A non-empty
/// response yielding zero parseable lines means the model ignored the format
/// (common with free/OpenRouter models) — that is an error, not an empty gap
/// list; returning `Ok(vec![])` surfaces downstream as a misleading
/// "No gaps found".
fn parse_gaps(text: &str, topic_id: &str) -> Result<Vec<KnowledgeGap>> {
    let gaps: Vec<KnowledgeGap> = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '|');
            let gap_type = match parts.next()?.trim() {
                "MissingLiterature" => GapType::MissingLiterature,
                "UnansweredQuestion" => GapType::UnansweredQuestion,
                "MethodologyGap" => GapType::MethodologyGap,
                "ConnectionGap" => GapType::ConnectionGap,
                _ => return None,
            };
            let desc = parts.next()?.trim().to_string();
            Some(KnowledgeGap::new(desc, topic_id.to_string(), gap_type))
        })
        .collect();
    if gaps.is_empty() && !text.trim().is_empty() {
        return Err(ResearchError::Source(format!(
            "gap analysis response matched no 'GAP_TYPE|description' lines; \
             model ignored the format. Response start: {:?}",
            text.chars().take(300).collect::<String>()
        )));
    }
    Ok(gaps)
}

#[async_trait]
impl ResearchEngine for LlmResearchEngine {
    async fn analyze_gaps(&self, topic_id: &str) -> Result<Vec<KnowledgeGap>> {
        let topic = self.store.get_topic(topic_id)?;
        let topic_name = topic.as_ref().map(|t| t.name.as_str()).unwrap_or("unknown");

        let Some(config) = &self.config else {
            return Ok(vec![
                KnowledgeGap::new(
                    format!("No comprehensive survey exists for '{topic_name}'"),
                    topic_id.to_string(),
                    GapType::MissingLiterature,
                ),
                KnowledgeGap::new(
                    format!("Reproducibility of key experiments in '{topic_name}' is unclear"),
                    topic_id.to_string(),
                    GapType::MethodologyGap,
                ),
            ]);
        };

        let context = self.gap_context_for_topic(topic_id)?;

        let prompt = format!(
            "Topic: {topic_name}\n\nPapers:\n{context}\n\n\
             Identify 3-5 specific knowledge gaps. \
             For each gap output one line: GAP_TYPE|description\n\
             GAP_TYPE must be one of: MissingLiterature, UnansweredQuestion, MethodologyGap, ConnectionGap"
        );

        let client = Self::make_client(config)?;
        let response = client
            .complete(LLMRequest {
                system: Some(
                    "You are a research analyst identifying knowledge gaps in academic literature."
                        .into(),
                ),
                messages: vec![ChatMessage::user(prompt)],
                temperature: 0.3,
                max_tokens: Some(512),
                ..LLMRequest::default()
            })
            .await
            .map_err(|e| ResearchError::Source(e.to_string()))?;

        let text = sanitize_output(&response.content);

        parse_gaps(&text, topic_id)
    }

    async fn generate_report(&self, title: &str, topic_ids: &[String]) -> Result<ResearchReport> {
        let mut report = ResearchReport::new(title.to_string(), topic_ids.to_vec());

        let Some(config) = &self.config else {
            report.sections.push(ReportSection {
                heading: "Introduction".into(),
                content: format!(
                    "This report covers {} topic(s): {}.",
                    topic_ids.len(),
                    topic_ids.join(", ")
                ),
            });
            report.sections.push(ReportSection {
                heading: "Knowledge Gaps".into(),
                content: "Gap analysis requires LLM configuration.".into(),
            });
            report.sections.push(ReportSection {
                heading: "Recommendations".into(),
                content: "Configure an LLM provider to generate recommendations.".into(),
            });
            return Ok(report);
        };

        let mut context = format!(
            "Report title: {title}\nTopics: {}\n\n",
            topic_ids.join(", ")
        );
        for topic_id in topic_ids {
            if let Some(topic) = self.store.get_topic(topic_id)? {
                context.push_str(&format!("## Topic: {}\n", topic.name));
                let papers_block = self.report_context_for_topic(topic_id)?;
                context.push_str(&papers_block);
                context.push('\n');
            }
        }

        let sections_to_write = [
            (
                "Introduction",
                "Write a 1-2 paragraph introduction covering the research area and scope.",
            ),
            (
                "Knowledge Gaps",
                "Identify and describe 3-5 specific knowledge gaps based on the papers.",
            ),
            (
                "Recommendations",
                "Provide 3-5 concrete, actionable research recommendations.",
            ),
        ];

        let client = Self::make_client(config)?;
        for (heading, instruction) in &sections_to_write {
            let prompt = format!("{context}\n\nTask: {instruction}");
            let response = client
                .complete(LLMRequest {
                    system: Some(
                        "You are an expert research analyst. Be specific and evidence-based."
                            .into(),
                    ),
                    messages: vec![ChatMessage::user(prompt)],
                    temperature: 0.4,
                    max_tokens: Some(512),
                    ..LLMRequest::default()
                })
                .await
                .map_err(|e| ResearchError::Source(e.to_string()))?;
            report.sections.push(ReportSection {
                heading: heading.to_string(),
                content: sanitize_output(&response.content),
            });
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_store::SqliteStore;
    use crate::domain::paper::Paper;
    use crate::domain::research_topic::ResearchTopic;

    fn setup() -> (Box<dyn IndexStore>, String) {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("Transformers".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();
        (Box::new(store), topic_id)
    }

    /// Build a store with two topics, one linked paper each, plus an unlinked
    /// paper. Returns the boxed store and the three paper titles so tests can
    /// assert on context contents without holding the paper structs.
    fn setup_with_papers() -> (
        Box<dyn IndexStore>,
        String, // topic A id
        String, // topic B id
        String, // paper A title
        String, // paper B title
        String, // unlinked paper title
    ) {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic_a = ResearchTopic::new("Transformers".into());
        let topic_b = ResearchTopic::new("Diffusion".into());
        let topic_a_id = topic_a.id.clone();
        let topic_b_id = topic_b.id.clone();
        store.insert_topic(&topic_a).unwrap();
        store.insert_topic(&topic_b).unwrap();

        let mut paper_a = Paper::new("Attention Is All You Need".into());
        paper_a.abstract_text = "Transformer architecture for sequence modeling".into();
        let mut paper_b = Paper::new("Denoising Diffusion Probabilistic Models".into());
        paper_b.abstract_text = "Generative models via iterative denoising".into();
        let mut paper_unlinked = Paper::new("Unlinked Graph Neural Net Survey".into());
        paper_unlinked.abstract_text = "Must never appear in a topic-scoped context".into();

        let title_a = paper_a.title.clone();
        let title_b = paper_b.title.clone();
        let title_unlinked = paper_unlinked.title.clone();
        store.insert_paper(&paper_a).unwrap();
        store.insert_paper(&paper_b).unwrap();
        store.insert_paper(&paper_unlinked).unwrap();
        store
            .link_paper_to_topic(&paper_a.id, &topic_a_id, 0.9)
            .unwrap();
        store
            .link_paper_to_topic(&paper_b.id, &topic_b_id, 0.9)
            .unwrap();

        (
            Box::new(store),
            topic_a_id,
            topic_b_id,
            title_a,
            title_b,
            title_unlinked,
        )
    }

    #[tokio::test]
    async fn analyze_gaps_fallback_returns_heuristic() {
        let (store, topic_id) = setup();
        let engine = LlmResearchEngine::new(store);
        let gaps = engine.analyze_gaps(&topic_id).await.unwrap();
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].topic_id, topic_id);
    }

    #[tokio::test]
    async fn generate_report_fallback_returns_sections() {
        let (store, topic_id) = setup();
        let engine = LlmResearchEngine::new(store);
        let report = engine
            .generate_report("Test Report", &[topic_id])
            .await
            .unwrap();
        assert_eq!(report.title, "Test Report");
        assert_eq!(report.sections.len(), 3);
        assert!(report.to_markdown().contains("# Test Report"));
    }

    #[test]
    fn gap_context_scopes_to_requested_topic() {
        // Regression for the bug where analyze_gaps called list_papers(Some(20))
        // and fed arbitrary papers to the LLM. The context must contain only
        // papers linked to the requested topic.
        let (store, topic_a_id, topic_b_id, title_a, title_b, title_unlinked) = setup_with_papers();
        let engine = LlmResearchEngine::new(store);

        let ctx_a = engine.gap_context_for_topic(&topic_a_id).unwrap();
        assert!(
            ctx_a.contains(&title_a),
            "topic A context must include its own paper"
        );
        assert!(
            !ctx_a.contains(&title_b),
            "must not leak another topic's paper"
        );
        assert!(
            !ctx_a.contains(&title_unlinked),
            "must not leak an unlinked paper"
        );

        let ctx_b = engine.gap_context_for_topic(&topic_b_id).unwrap();
        assert!(ctx_b.contains(&title_b));
        assert!(!ctx_b.contains(&title_a));
        assert!(!ctx_b.contains(&title_unlinked));
    }

    #[test]
    fn gap_context_empty_topic_does_not_leak_arbitrary_papers() {
        // A topic with no linked papers must yield an explicit placeholder, not
        // a dump of arbitrary recent papers from the store.
        let (store, _topic_a_id, _topic_b_id, title_a, title_b, title_unlinked) =
            setup_with_papers();
        let engine = LlmResearchEngine::new(store);

        let empty_topic = ResearchTopic::new("Brand New Topic".into());
        let empty_id = empty_topic.id.clone();
        engine.store.insert_topic(&empty_topic).unwrap();

        let ctx = engine.gap_context_for_topic(&empty_id).unwrap();
        assert!(
            ctx.contains("no papers indexed"),
            "empty topic must produce the placeholder, got: {ctx}"
        );
        assert!(!ctx.contains(&title_a));
        assert!(!ctx.contains(&title_b));
        assert!(!ctx.contains(&title_unlinked));
    }

    #[test]
    fn report_context_scopes_to_requested_topic() {
        // Regression for the bug where generate_report called list_papers(Some(5))
        // inside the per-topic loop, emitting the SAME arbitrary papers for every
        // topic. Each topic's block must contain only its own papers.
        let (store, topic_a_id, topic_b_id, title_a, title_b, title_unlinked) = setup_with_papers();
        let engine = LlmResearchEngine::new(store);

        let block_a = engine.report_context_for_topic(&topic_a_id).unwrap();
        assert!(block_a.contains(&title_a));
        assert!(!block_a.contains(&title_b));
        assert!(!block_a.contains(&title_unlinked));

        let block_b = engine.report_context_for_topic(&topic_b_id).unwrap();
        assert!(block_b.contains(&title_b));
        assert!(!block_b.contains(&title_a));
        assert!(!block_b.contains(&title_unlinked));
    }

    #[test]
    fn report_context_empty_topic_does_not_leak_arbitrary_papers() {
        let (store, _topic_a_id, _topic_b_id, title_a, title_b, title_unlinked) =
            setup_with_papers();
        let engine = LlmResearchEngine::new(store);

        let empty_topic = ResearchTopic::new("Brand New Topic".into());
        let empty_id = empty_topic.id.clone();
        engine.store.insert_topic(&empty_topic).unwrap();

        let block = engine.report_context_for_topic(&empty_id).unwrap();
        assert!(
            block.contains("no papers indexed"),
            "empty topic must produce the placeholder, got: {block}"
        );
        assert!(!block.contains(&title_a));
        assert!(!block.contains(&title_b));
        assert!(!block.contains(&title_unlinked));
    }

    #[test]
    fn parse_gaps_reads_valid_lines() {
        let gaps = parse_gaps(
            "MissingLiterature|no survey of X\nUnansweredQuestion| does Y hold? \n",
            "topic-1",
        )
        .unwrap();
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].description, "no survey of X");
        assert_eq!(gaps[1].topic_id, "topic-1");
    }

    #[test]
    fn parse_gaps_empty_response_is_ok_empty() {
        assert!(parse_gaps("", "t").unwrap().is_empty());
        assert!(parse_gaps("  \n ", "t").unwrap().is_empty());
    }

    #[test]
    fn parse_gaps_errors_on_unparseable_response() {
        // Regression: a prose answer that matches zero `GAP_TYPE|description`
        // lines used to become an empty vec, printed as "No gaps found".
        let err = parse_gaps(
            "Here are some gaps:\n1. Nobody has studied X\n2. Y is unclear",
            "t",
        );
        assert!(
            err.is_err(),
            "prose response must be an error, not empty ok"
        );
    }

    #[test]
    fn report_context_includes_all_linked_papers_within_budget() {
        // Regression: the report context capped linked papers at a fixed 5,
        // silently dropping the rest — with equal relevance scores the
        // dropped ones were whichever the store sorted last.
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("Many papers".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();

        let titles: Vec<String> = (0..7)
            .map(|i| {
                let mut paper = Paper::new(format!("Paper number {i}"));
                paper.abstract_text = "short abstract".into();
                store.insert_paper(&paper).unwrap();
                store
                    .link_paper_to_topic(&paper.id, &topic_id, 0.5)
                    .unwrap();
                paper.title
            })
            .collect();

        let engine = LlmResearchEngine::new(Box::new(store));
        let block = engine.report_context_for_topic(&topic_id).unwrap();
        for title in &titles {
            assert!(
                block.contains(title.as_str()),
                "linked paper '{title}' missing from report context, got: {block}"
            );
        }
    }
}
