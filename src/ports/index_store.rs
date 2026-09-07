use crate::domain::knowledge_gap::KnowledgeGap;
use crate::domain::paper::{Paper, PaperStatus, Rating, ReadingStatus};
use crate::domain::research_report::ResearchReport;
use crate::domain::research_state::ResearchState;
use crate::domain::research_topic::ResearchTopic;
use crate::error::Result;

pub trait IndexStore: Send + Sync {
    // Papers
    fn insert_paper(&self, paper: &Paper) -> Result<()>;
    fn get_paper(&self, id: &str) -> Result<Option<Paper>>;
    fn update_paper_status(&self, id: &str, status: PaperStatus) -> Result<()>;
    fn update_reading_status(&self, id: &str, status: ReadingStatus) -> Result<()>;
    fn update_rating(&self, id: &str, rating: Rating) -> Result<()>;
    fn clear_rating(&self, id: &str) -> Result<()>;
    fn search_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>>;
    fn list_papers(&self, limit: Option<usize>) -> Result<Vec<Paper>>;
    /// Papers explicitly linked to `topic_id` via `topic_papers`, most relevant
    /// first. Used so analysis/report builders scope LLM context to the
    /// requested topic rather than arbitrary recent papers.
    fn list_papers_by_topic(&self, topic_id: &str, limit: Option<usize>) -> Result<Vec<Paper>>;

    // Topics
    fn insert_topic(&self, topic: &ResearchTopic) -> Result<()>;
    fn get_topic(&self, id: &str) -> Result<Option<ResearchTopic>>;
    fn list_topics(&self) -> Result<Vec<ResearchTopic>>;
    fn link_paper_to_topic(&self, paper_id: &str, topic_id: &str, relevance: f32) -> Result<()>;

    // Knowledge gaps
    fn insert_gap(&self, gap: &KnowledgeGap) -> Result<()>;
    fn list_gaps(&self, topic_id: Option<&str>) -> Result<Vec<KnowledgeGap>>;

    // Research state
    fn get_research_state(&self, topic_id: &str) -> Result<Option<ResearchState>>;
    fn update_research_state(&self, state: &ResearchState) -> Result<()>;

    // Reports
    fn insert_report(&self, report: &ResearchReport) -> Result<()>;
    fn list_reports(&self, limit: Option<usize>) -> Result<Vec<ResearchReport>>;

    // Index management
    fn rebuild_index(&self) -> Result<()>;
    fn init_schema(&self) -> Result<()>;
}
