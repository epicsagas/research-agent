use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchTopic {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parent_topic_id: Option<String>,
    pub depth: u32,
    pub priority: f32,
    pub created_at: String,
}

impl ResearchTopic {
    pub fn new(name: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: String::new(),
            parent_topic_id: None,
            depth: 0,
            priority: 0.5,
            created_at: now,
        }
    }

    /// Create a sub-topic under `parent`, setting `parent_topic_id` and computing
    /// `depth` from the parent. This is the single source of truth for the depth
    /// rule, so the CLI and tests cannot diverge from it.
    pub fn new_subtopic(name: String, parent: &ResearchTopic) -> Self {
        let mut topic = Self::new(name);
        topic.parent_topic_id = Some(parent.id.clone());
        topic.depth = parent.depth + 1;
        topic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_new_generates_fields() {
        let t = ResearchTopic::new("Transformer Architectures".into());
        assert_eq!(t.name, "Transformer Architectures");
        assert!(!t.id.is_empty());
        assert!(t.parent_topic_id.is_none());
        assert_eq!(t.depth, 0);
    }

    #[test]
    fn new_subtopic_inherits_parent_and_depth() {
        let parent = ResearchTopic::new("ML".into());
        let child = ResearchTopic::new_subtopic("Deep Learning".into(), &parent);
        assert_eq!(child.parent_topic_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.depth, parent.depth + 1);
    }
}
