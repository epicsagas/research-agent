use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GapType {
    MissingLiterature,
    UnansweredQuestion,
    MethodologyGap,
    ConnectionGap,
}

impl GapType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingLiterature => "missing_literature",
            Self::UnansweredQuestion => "unanswered_question",
            Self::MethodologyGap => "methodology_gap",
            Self::ConnectionGap => "connection_gap",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "unanswered_question" => Self::UnansweredQuestion,
            "methodology_gap" => Self::MethodologyGap,
            "connection_gap" => Self::ConnectionGap,
            _ => Self::MissingLiterature,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    pub id: String,
    pub description: String,
    pub topic_id: String,
    pub gap_type: GapType,
    pub priority: f32,
    pub discovered_at: String,
}

impl KnowledgeGap {
    pub fn new(description: String, topic_id: String, gap_type: GapType) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            topic_id,
            gap_type,
            priority: 0.5,
            discovered_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_type_roundtrip() {
        let types = [
            GapType::MissingLiterature,
            GapType::UnansweredQuestion,
            GapType::MethodologyGap,
            GapType::ConnectionGap,
        ];
        for t in &types {
            assert_eq!(GapType::from_str_lossy(t.as_str()), *t);
        }
    }

    #[test]
    fn gap_new() {
        let g = KnowledgeGap::new(
            "Missing survey papers".into(),
            "topic-1".into(),
            GapType::MissingLiterature,
        );
        assert_eq!(g.description, "Missing survey papers");
        assert_eq!(g.topic_id, "topic-1");
        assert!(!g.id.is_empty());
    }
}
