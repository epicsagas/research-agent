use crate::application::gap_analyzer::TopicBrief;
use crate::domain::research_report::{ReportSection, ResearchReport};
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::research_engine::ResearchEngine;
use serde::Serialize;

/// Source material for an agent-authored report: one brief per topic. The
/// agent drafts the markdown with its own model and stores it via
/// [`save_report`].
#[derive(Debug, Serialize)]
pub struct ReportMaterial {
    pub topics: Vec<TopicBrief>,
}

pub fn collect_material(store: &dyn IndexStore, topic_ids: &[String]) -> Result<ReportMaterial> {
    let topics = topic_ids
        .iter()
        .map(|id| crate::application::gap_analyzer::collect_brief(store, id))
        .collect::<Result<Vec<_>>>()?;
    Ok(ReportMaterial { topics })
}

/// Split markdown into sections on `## ` headings (content before the first
/// heading becomes the single intro section).
fn sections_from_markdown(markdown: &str) -> Vec<ReportSection> {
    let mut sections: Vec<ReportSection> = Vec::new();
    for line in markdown.lines() {
        if let Some(heading) = line.strip_prefix("## ").map(str::trim) {
            sections.push(ReportSection {
                heading: heading.to_string(),
                content: String::new(),
            });
        } else if let Some(last) = sections.last_mut() {
            if !last.content.is_empty() {
                last.content.push('\n');
            }
            last.content.push_str(line);
        } else if !line.trim().is_empty() {
            sections.push(ReportSection {
                heading: String::new(),
                content: line.to_string(),
            });
        }
    }
    sections
}

/// Store an agent-authored markdown report over the given topics.
pub fn save_report(
    store: &dyn IndexStore,
    title: &str,
    topic_ids: &[String],
    markdown: &str,
) -> Result<ResearchReport> {
    let mut report = ResearchReport::new(title.to_string(), topic_ids.to_vec());
    report.sections = sections_from_markdown(markdown);
    store.insert_report(&report)?;
    Ok(report)
}

pub struct ReportGenerator<'a> {
    engine: &'a dyn ResearchEngine,
    store: &'a dyn IndexStore,
}

impl<'a> ReportGenerator<'a> {
    pub fn new(engine: &'a dyn ResearchEngine, store: &'a dyn IndexStore) -> Self {
        Self { engine, store }
    }

    pub async fn generate(&self, title: &str, topic_ids: &[String]) -> Result<ResearchReport> {
        let report = self.engine.generate_report(title, topic_ids).await?;
        self.store.insert_report(&report)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::llm_research_engine::LlmResearchEngine;
    use crate::adapters::sqlite_store::SqliteStore;
    use crate::domain::research_topic::ResearchTopic;

    #[test]
    fn save_report_splits_markdown_sections() {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("T".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();

        let report = save_report(
            &store,
            "Survey",
            &[topic_id],
            "# Survey\n\nIntro line.\n\n## Methods\n\nWe reviewed.\n\n## Findings\n\nThree gaps.",
        )
        .unwrap();
        assert_eq!(report.title, "Survey");
        // Intro paragraph becomes its own section, then one per '## ' heading.
        assert_eq!(report.sections.len(), 3);
        assert_eq!(report.sections[1].heading, "Methods");
        assert!(report.sections[2].content.contains("Three gaps"));

        assert_eq!(store.list_reports(None).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn generate_stores_report() {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("Test".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();

        let engine = LlmResearchEngine::new(Box::new(SqliteStore::open_in_memory().unwrap()));
        let generator = ReportGenerator::new(&engine, &store);

        let report = generator.generate("Report", &[topic_id]).await.unwrap();
        assert_eq!(report.title, "Report");

        let stored = store.list_reports(None).unwrap();
        assert_eq!(stored.len(), 1);
    }
}
