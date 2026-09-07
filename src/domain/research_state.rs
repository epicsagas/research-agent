use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub topic_id: String,
    pub papers_read: i64,
    pub papers_queued: i64,
    pub gaps_identified: i64,
    pub coverage_score: f32,
    pub last_updated: String,
}

impl ResearchState {
    pub fn new(topic_id: String) -> Self {
        Self {
            topic_id,
            papers_read: 0,
            papers_queued: 0,
            gaps_identified: 0,
            coverage_score: 0.0,
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_new_defaults() {
        let s = ResearchState::new("topic-1".into());
        assert_eq!(s.topic_id, "topic-1");
        assert_eq!(s.papers_read, 0);
        assert_eq!(s.papers_queued, 0);
        assert_eq!(s.coverage_score, 0.0);
    }
}
