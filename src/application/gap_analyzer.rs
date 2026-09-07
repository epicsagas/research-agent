use crate::domain::knowledge_gap::KnowledgeGap;
use crate::domain::research_state::ResearchState;
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::research_engine::ResearchEngine;

pub struct GapAnalyzer<'a> {
    engine: &'a dyn ResearchEngine,
    store: &'a dyn IndexStore,
}

impl<'a> GapAnalyzer<'a> {
    pub fn new(engine: &'a dyn ResearchEngine, store: &'a dyn IndexStore) -> Self {
        Self { engine, store }
    }

    pub async fn analyze(&self, topic_id: &str) -> Result<Vec<KnowledgeGap>> {
        let gaps = self.engine.analyze_gaps(topic_id).await?;

        for gap in &gaps {
            self.store.insert_gap(gap)?;
        }

        let state = ResearchState {
            topic_id: topic_id.to_string(),
            gaps_identified: gaps.len() as i64,
            ..ResearchState::new(topic_id.to_string())
        };
        self.store.update_research_state(&state)?;

        Ok(gaps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::llm_research_engine::LlmResearchEngine;
    use crate::adapters::sqlite_store::SqliteStore;
    use crate::domain::research_topic::ResearchTopic;

    #[tokio::test]
    async fn analyze_stores_gaps() {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("Test".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();

        let engine = LlmResearchEngine::new(Box::new(SqliteStore::open_in_memory().unwrap()));
        let analyzer = GapAnalyzer::new(&engine, &store);

        let gaps = analyzer.analyze(&topic_id).await.unwrap();
        assert_eq!(gaps.len(), 2);

        let stored = store.list_gaps(Some(&topic_id)).unwrap();
        assert_eq!(stored.len(), 2);
    }
}
