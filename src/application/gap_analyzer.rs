use crate::domain::knowledge_gap::{GapType, KnowledgeGap};
use crate::domain::paper::Paper;
use crate::domain::research_state::ResearchState;
use crate::domain::research_topic::ResearchTopic;
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::research_engine::ResearchEngine;
use serde::Serialize;

/// Everything an agent needs to reason about one topic's coverage, collected
/// without any LLM call. The calling agent analyzes the brief with its own
/// model and reports back through [`record_gaps`].
#[derive(Debug, Serialize)]
pub struct TopicBrief {
    pub topic: ResearchTopic,
    pub state: Option<ResearchState>,
    pub papers: Vec<Paper>,
    pub recorded_gaps: Vec<KnowledgeGap>,
}

/// Collect the brief for one topic. Store-only; safe to call from hosts with
/// no LLM configured.
pub fn collect_brief(store: &dyn IndexStore, topic_id: &str) -> Result<TopicBrief> {
    let topic = store.get_topic(topic_id)?.ok_or_else(|| {
        crate::error::ResearchError::Validation(format!("topic '{topic_id}' not found"))
    })?;
    Ok(TopicBrief {
        state: store.get_research_state(topic_id)?,
        papers: store.list_papers_by_topic(topic_id, None)?,
        recorded_gaps: store.list_gaps(Some(topic_id))?,
        topic,
    })
}

/// Persist agent-identified gaps for a topic and refresh the topic's gap
/// counter. Descriptions must be unique enough to be useful; type defaults to
/// `missing_literature` when absent or unrecognized, priority clamps to 0..=1.
pub fn record_gaps(
    store: &dyn IndexStore,
    topic_id: &str,
    gaps: &[(String, Option<String>, Option<f32>)],
) -> Result<Vec<KnowledgeGap>> {
    let mut saved = Vec::with_capacity(gaps.len());
    for (description, gap_type, priority) in gaps {
        if description.trim().is_empty() {
            continue;
        }
        let gap_type = gap_type
            .as_deref()
            .map(GapType::from_str_lossy)
            .unwrap_or(GapType::MissingLiterature);
        let mut gap = KnowledgeGap::new(
            description.trim().to_string(),
            topic_id.to_string(),
            gap_type,
        );
        if let Some(p) = priority {
            gap.priority = p.clamp(0.0, 1.0);
        }
        store.insert_gap(&gap)?;
        saved.push(gap);
    }

    if !saved.is_empty() {
        let mut state = store
            .get_research_state(topic_id)?
            .unwrap_or_else(|| ResearchState::new(topic_id.to_string()));
        state.gaps_identified = store.list_gaps(Some(topic_id))?.len() as i64;
        state.last_updated = chrono::Utc::now().to_rfc3339();
        store.update_research_state(&state)?;
    }
    Ok(saved)
}

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

    #[test]
    fn collect_brief_gathers_topic_data() {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("Test".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();
        let mut paper = crate::domain::paper::Paper::new("P".into());
        store.insert_paper(&paper).unwrap();
        paper.reading_status = crate::domain::paper::ReadingStatus::Queued;
        let _ = paper;
        store
            .link_paper_to_topic(&store.list_papers(None).unwrap()[0].id, &topic_id, 0.9)
            .unwrap();

        let brief = collect_brief(&store, &topic_id).unwrap();
        assert_eq!(brief.topic.id, topic_id);
        assert_eq!(brief.papers.len(), 1);
        assert_eq!(brief.recorded_gaps.len(), 0);

        assert!(collect_brief(&store, "missing").is_err());
    }

    #[test]
    fn record_gaps_persists_and_updates_state() {
        let store = SqliteStore::open_in_memory().unwrap();
        let topic = ResearchTopic::new("T".into());
        let topic_id = topic.id.clone();
        store.insert_topic(&topic).unwrap();

        let saved = record_gaps(
            &store,
            &topic_id,
            &[
                ("missing optimization literature".into(), None, Some(1.2)),
                ("".into(), None, None),
                (
                    "open question".into(),
                    Some("unanswered_question".into()),
                    None,
                ),
            ],
        )
        .unwrap();
        // Empty descriptions are skipped; priority clamps into 0..=1.
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].priority, 1.0);
        assert_eq!(
            saved[1].gap_type,
            crate::domain::knowledge_gap::GapType::UnansweredQuestion
        );

        let stored = store.list_gaps(Some(&topic_id)).unwrap();
        assert_eq!(stored.len(), 2);
        let state = store.get_research_state(&topic_id).unwrap().unwrap();
        assert_eq!(state.gaps_identified, 2);
    }

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
