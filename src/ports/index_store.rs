use crate::domain::anchor::Anchor;
use crate::domain::citation::Citation;
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
    /// Look up a paper by its DOI (exact match). Used by the import pipeline
    /// to skip records that are already in the library.
    fn find_paper_by_doi(&self, doi: &str) -> Result<Option<Paper>>;
    /// Look up a paper by its OpenAlex work id (`W…`). Used by the reference
    /// graph to dedupe hydrated referenced papers on re-runs.
    fn find_paper_by_openalex_id(&self, openalex_id: &str) -> Result<Option<Paper>>;
    /// Look up a paper by its stored source PDF path. Second identity key:
    /// the same file re-ingested must not become a second row.
    fn find_paper_by_pdf_path(&self, path: &str) -> Result<Option<Paper>>;
    /// Look up a paper by title, compared in normalized form
    /// ([`crate::domain::paper::normalize_title`]). Last identity key, for
    /// papers carrying neither a DOI nor a pdf_path. `title` must already be
    /// normalized.
    fn find_paper_by_title(&self, title: &str) -> Result<Option<Paper>>;
    /// Store (or replace) the extracted full body text of a paper. Kept out of
    /// the `Paper` domain type so MCP/tool responses never carry megabytes of
    /// body text.
    fn set_paper_body(&self, paper_id: &str, body: &str) -> Result<()>;
    /// Fetch the stored body text of a paper, if any.
    fn get_paper_body(&self, paper_id: &str) -> Result<Option<String>>;
    /// Record where the paper's PDF lives on disk (a locally ingested file or
    /// a downloaded arXiv PDF), so `reingest` can re-extract without
    /// redownloading.
    fn set_paper_pdf_path(&self, paper_id: &str, path: &str) -> Result<()>;
    /// (rowid, embeddable text) for every paper — the vector index corpus.
    /// The rowid is the stable join key between the store and the vector file.
    fn vector_corpus(&self) -> Result<Vec<(i64, String)>>;
    /// Resolve a vector-index rowid back to its paper.
    fn paper_by_rowid(&self, rowid: i64) -> Result<Option<Paper>>;
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

    // Citations
    /// Insert citation edges, skipping pairs already stored. Returns how many
    /// were newly inserted.
    fn insert_citations(&self, citations: &[Citation]) -> Result<usize>;
    /// Reference edges originating from `paper_id`, insertion order.
    fn citations_for_paper(&self, paper_id: &str) -> Result<Vec<Citation>>;
    /// Citation edges pointing at `paper_id` (works citing it), insertion
    /// order.
    fn citations_citing_paper(&self, paper_id: &str) -> Result<Vec<Citation>>;
    /// Set the `context` label on existing citation edges. Pairs with no
    /// stored edge are ignored. Returns how many rows changed.
    fn set_citation_contexts(&self, citations: &[Citation]) -> Result<usize>;

    /// Body-text matches for `query`, each carrying the matching snippet and
    /// where in the document it sits. Papers whose body is not stored (no PDF
    /// ingest) cannot match. `paper_id` scopes the search to one paper; without
    /// it the whole library is searched.
    fn search_body_evidence(
        &self,
        query: &str,
        paper_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BodyEvidence>>;

    // Index management
    fn rebuild_index(&self) -> Result<()>;
    fn init_schema(&self) -> Result<()>;
}

/// One body-text match: which paper, the surrounding text, and where it sits.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BodyEvidence {
    pub paper_id: String,
    pub title: String,
    /// Matching text with surrounding context; match terms are bracketed.
    pub snippet: String,
    pub anchor: Anchor,
}
