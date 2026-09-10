use async_trait::async_trait;

use crate::domain::paper::Paper;
use crate::domain::knowledge_gap::KnowledgeGap;
use crate::domain::research_report::ResearchReport;
use crate::error::Result;

#[async_trait]
pub trait ResearchEngine: Send + Sync {
    /// Analyze indexed papers for a topic and identify knowledge gaps.
    async fn analyze_gaps(&self, topic_id: &str) -> Result<Vec<KnowledgeGap>>;

    /// Generate a research report for one or more topics.
    /// Generate search keywords for a batch of papers. Returns
    /// `(paper_id, "kw1; kw2; kw3")` pairs; papers the model skipped are simply
    /// absent. An engine with no `[llm]` config returns an empty vec — that is
    /// a normal state (the host agent does the work instead), not an error.
    async fn extract_keywords(&self, papers: &[Paper]) -> Result<Vec<(String, String)>>;
    async fn generate_report(&self, title: &str, topic_ids: &[String]) -> Result<ResearchReport>;
}
