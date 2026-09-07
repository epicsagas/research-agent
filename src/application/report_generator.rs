use crate::domain::research_report::ResearchReport;
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::research_engine::ResearchEngine;

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
