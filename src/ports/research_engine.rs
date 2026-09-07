use async_trait::async_trait;

use crate::domain::knowledge_gap::KnowledgeGap;
use crate::domain::research_report::ResearchReport;
use crate::error::Result;

#[async_trait]
pub trait ResearchEngine: Send + Sync {
    /// Analyze indexed papers for a topic and identify knowledge gaps.
    async fn analyze_gaps(&self, topic_id: &str) -> Result<Vec<KnowledgeGap>>;

    /// Generate a research report for one or more topics.
    async fn generate_report(&self, title: &str, topic_ids: &[String]) -> Result<ResearchReport>;
}
