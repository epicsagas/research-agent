//! The research-agent stdio MCP server (`research serve`).
//!
//! Wraps research-agent's application layer as MCP tools so an LLM agent can
//! drive the whole ingest/query/gaps/report flow. Each `#[tool]` method
//! calls the application adapters directly; domain data is returned as opaque
//! `serde_json::Value` (domain types derive `Serialize` but not `JsonSchema`).
//! Tool bodies mirror the CLI handlers in `src/main.rs` but return JSON instead
//! of printing to stdout; the two interfaces share the same application layer.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::{ServerHandler, ServiceExt};
use tokio::io::AsyncReadExt;

use super::guard;
use serde_json::{Value, json};

use crate::adapters::arxiv_source::ArxivSource;
use crate::adapters::pdf_source::PdfSource;
use crate::adapters::semantic_scholar_source::SemanticScholarSource;
use crate::adapters::sqlite_store::SqliteStore;
use crate::application::gap_analyzer::GapAnalyzer;
use crate::application::ingest_pipeline::IngestPipeline;
use crate::application::report_generator::ReportGenerator;
use crate::composition::{load_config, make_llm_engine, open_store};
use crate::domain::paper::{Paper, Rating, ReadingStatus};
use crate::domain::research_topic::ResearchTopic;
use crate::error::ResearchError;
use crate::ports::index_store::IndexStore;

use super::params::*;

/// Fixed-at-startup runtime context shared (immutable) across all tool calls.
pub struct ResearchContext {
    /// Database path resolved once at startup (default or `--db` override).
    pub db_path: PathBuf,
}

/// The MCP server. Holds an immutable `Arc<ResearchContext>`; tool methods
/// borrow it.
#[derive(Clone)]
pub struct ResearchServer {
    ctx: Arc<ResearchContext>,
}

impl ResearchServer {
    pub fn new(ctx: ResearchContext) -> Self {
        Self { ctx: Arc::new(ctx) }
    }

    /// Run the stdio MCP server until the client disconnects.
    ///
    /// Returns a `String` error (Send + Sync) so the binary entry point can
    /// surface it via `anyhow`.
    pub async fn serve_stdio(self) -> Result<(), String> {
        // Antigravity-style clients probe with non-MCP requests before
        // `initialize`; rmcp aborts the handshake on those, so consume and
        // answer them first. `None` = client hung up before handshaking.
        let Some(first_line) = guard::read_until_forwardable().await else {
            return Ok(());
        };
        // Replay the held line into the transport, then hand stdin over.
        let stdin = std::io::Cursor::new(first_line).chain(tokio::io::stdin());
        let service = self
            .serve((stdin, tokio::io::stdout()))
            .await
            .map_err(|e| format!("MCP serve init failed: {e}"))?;
        service
            .waiting()
            .await
            .map_err(|e| format!("MCP serve stopped: {e}"))?;
        Ok(())
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────

/// Successful tool result carrying a JSON value as a text content block.
fn ok_value(v: Value) -> CallToolResult {
    CallToolResult::success(vec![rmcp::model::ContentBlock::text(v.to_string())])
}

/// Error tool result. Maps research-agent errors to human-readable text; the
/// MCP layer surfaces this as an `is_error` result the agent can read.
fn err_result(e: ResearchError) -> CallToolResult {
    let kind = match &e {
        ResearchError::NotFound(_) | ResearchError::Duplicate(_) | ResearchError::Validation(_) => {
            "invalid_params"
        }
        ResearchError::Config(_) => "dependency_missing",
        // arXiv / Semantic Scholar fetch failure: upstream service, not a
        // local dependency.
        ResearchError::Source(_) => "upstream_error",
        _ => "internal_error", // Database, Io, Serialization
    };
    CallToolResult::error(vec![rmcp::model::ContentBlock::text(format!(
        "[{kind}] {e}"
    ))])
}

/// Convert a `Result<T>` (where T: Serialize) into a tool result.
macro_rules! tool_result {
    ($expr:expr) => {
        match $expr {
            Ok(v) => ok_value(serde_json::to_value(&v).unwrap_or(Value::Null)),
            Err(e) => err_result(e),
        }
    };
}

// ─── ingest helpers ─────────────────────────────────────────────────────────

/// Tool-shaped outcome: either the value or an already-built error response.
type ToolOutcome<T> = Result<T, CallToolResult>;

/// Fetch and persist papers for the requested source(s). Returns the papers
/// plus the count of individually skipped files (PDFs only).
async fn ingest_papers(store: &SqliteStore, p: &IngestParams) -> ToolOutcome<(Vec<Paper>, usize)> {
    if p.source == "pdf" {
        ingest_pdfs(store, p)
    } else {
        ingest_remote(store, p).await
    }
}

/// Ingest local PDFs, skipping individual unreadable files (counted, not
/// fatal — reported back in the tool response).
fn ingest_pdfs(store: &SqliteStore, p: &IngestParams) -> ToolOutcome<(Vec<Paper>, usize)> {
    let Some(pdf_path) = &p.path else {
        return Err(err_result(ResearchError::Validation(
            "path is required for source=pdf".into(),
        )));
    };
    let src = PdfSource::new();
    let paths = PdfSource::collect_paths(std::path::Path::new(pdf_path)).map_err(err_result)?;
    let mut papers = Vec::new();
    let mut skipped = 0usize;
    for path in &paths {
        match src.ingest_file(path) {
            Ok(paper) => {
                store.insert_paper(&paper).map_err(err_result)?;
                papers.push(paper);
            }
            Err(_) => skipped += 1,
        }
    }
    Ok((papers, skipped))
}

/// Query arXiv and/or Semantic Scholar through the ingest pipeline. Remote
/// sources fail the whole call on error, so nothing is silently skipped.
async fn ingest_remote(store: &SqliteStore, p: &IngestParams) -> ToolOutcome<(Vec<Paper>, usize)> {
    let Some(q) = &p.query else {
        return Err(err_result(ResearchError::Validation(format!(
            "query is required for source={}",
            p.source
        ))));
    };
    let mut papers = Vec::new();
    if p.source == "arxiv" || p.source == "all" {
        let arxiv = ArxivSource::new();
        let fetched = IngestPipeline::new(&arxiv, store)
            .run(q, p.limit)
            .await
            .map_err(err_result)?;
        papers.extend(fetched);
    }
    if p.source == "s2" || p.source == "all" {
        let s2 = SemanticScholarSource::new();
        let fetched = IngestPipeline::new(&s2, store)
            .run(q, p.limit)
            .await
            .map_err(err_result)?;
        papers.extend(fetched);
    }
    Ok((papers, 0))
}

/// Link every ingested paper to the requested topic. A missing topic is an
/// error, matching the CLI behavior.
fn link_ingested_to_topic(
    store: &SqliteStore,
    papers: &[Paper],
    topic: &Option<String>,
) -> ToolOutcome<()> {
    let Some(topic_id) = topic else {
        return Ok(());
    };
    match store.get_topic(topic_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(err_result(ResearchError::NotFound(format!(
                "topic '{topic_id}'"
            ))));
        }
        Err(e) => return Err(err_result(e)),
    }
    for paper in papers {
        store
            .link_paper_to_topic(&paper.id, topic_id, paper.relevance_score)
            .map_err(err_result)?;
    }
    Ok(())
}

// ─── tools ──────────────────────────────────────────────────────────────────

#[tool_router]
impl ResearchServer {
    #[tool(
        description = "Initialize a research workspace: create the SQLite index schema and default config if missing. Returns the db path."
    )]
    pub fn init(&self) -> CallToolResult {
        let db_path = self.ctx.db_path.clone();
        // Materialize default config if absent (load_config writes it).
        if let Err(e) = load_config() {
            return err_result(e);
        }
        match open_store(&db_path) {
            Ok(store) => match store.init_schema() {
                Ok(()) => ok_value(json!({
                    "initialized": true,
                    "db": db_path.to_string_lossy(),
                })),
                Err(e) => err_result(e),
            },
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "Ingest papers from arXiv, Semantic Scholar, or local PDFs. source: arxiv|s2|all|pdf. For arxiv/s2/all a query is required; for pdf a path (file or dir) is required. Optionally link ingested papers to a topic. Network-heavy for arxiv/s2 (async)."
    )]
    pub async fn ingest(&self, Parameters(p): Parameters<IngestParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let (all_papers, skipped) = match ingest_papers(&store, &p).await {
            Ok((papers, skipped)) => (papers, skipped),
            Err(resp) => return resp,
        };
        if let Err(resp) = link_ingested_to_topic(&store, &all_papers, &p.topic) {
            return resp;
        }

        ok_value(json!({
            "ingested": all_papers.len(),
            "skipped": skipped,
            "source": p.source,
            "linked_topic": p.topic,
            "papers": serde_json::to_value(&all_papers).unwrap_or(Value::Null),
        }))
    }

    #[tool(description = "Force a full rebuild of the search index.")]
    pub fn index_rebuild(&self) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        tool_result!(store.rebuild_index().map(|_| json!({"rebuilt": true})))
    }

    #[tool(
        description = "Search the local paper index by query. Returns matching papers (id, title, authors, year, status)."
    )]
    pub fn query_papers(&self, Parameters(p): Parameters<QueryPapersParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        tool_result!(store.search_papers(&p.query, p.limit))
    }

    #[tool(
        description = "Run LLM knowledge-gap analysis for a topic. Requires a configured [llm] in config.toml. Returns identified gaps."
    )]
    pub async fn analyze_gaps(
        &self,
        Parameters(p): Parameters<AnalyzeGapsParams>,
    ) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let inner = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let engine = match make_llm_engine(inner) {
            Ok(e) => e,
            Err(e) => return err_result(e),
        };
        let analyzer = GapAnalyzer::new(&engine, &store);
        tool_result!(analyzer.analyze(&p.topic).await)
    }

    #[tool(description = "List recorded knowledge gaps, optionally filtered by topic id.")]
    pub fn list_gaps(&self, Parameters(p): Parameters<ListGapsParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        tool_result!(store.list_gaps(p.topic.as_deref()))
    }

    #[tool(
        description = "Generate a research report (markdown + metadata) over comma-separated topic ids. Requires a configured [llm]."
    )]
    pub async fn generate_report(
        &self,
        Parameters(p): Parameters<GenerateReportParams>,
    ) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let inner = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let engine = match make_llm_engine(inner) {
            Ok(e) => e,
            Err(e) => return err_result(e),
        };
        let generator = ReportGenerator::new(&engine, &store);
        let topic_ids: Vec<String> = p.topic.split(',').map(String::from).collect();
        match generator.generate(&p.title, &topic_ids).await {
            Ok(report) => ok_value(json!({
                "id": report.id,
                "title": report.title,
                "markdown": report.to_markdown(),
            })),
            Err(e) => err_result(e),
        }
    }

    #[tool(description = "List all research topics with their hierarchy depth.")]
    pub fn topics_list(&self) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        tool_result!(store.list_topics())
    }

    #[tool(description = "Add a research topic, optionally as a sub-topic of a parent.")]
    pub fn topic_add(&self, Parameters(p): Parameters<TopicAddParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let mut topic = match &p.parent {
            Some(parent_id) => match store.get_topic(parent_id) {
                Ok(Some(parent)) => ResearchTopic::new_subtopic(p.name, &parent),
                Ok(None) => {
                    return err_result(ResearchError::NotFound(format!(
                        "parent topic '{parent_id}'"
                    )));
                }
                Err(e) => return err_result(e),
            },
            None => ResearchTopic::new(p.name),
        };
        topic.description = p.description;
        let id = topic.id.clone();
        let depth = topic.depth;
        match store.insert_topic(&topic) {
            Ok(()) => ok_value(json!({ "id": id, "depth": depth })),
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "Research state overview: counts of topics/papers/gaps plus per-topic coverage."
    )]
    pub fn state(&self) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let topics = match store.list_topics() {
            Ok(t) => t,
            Err(e) => return err_result(e),
        };
        let papers = match store.list_papers(None) {
            Ok(p) => p,
            Err(e) => return err_result(e),
        };
        let gaps = match store.list_gaps(None) {
            Ok(g) => g,
            Err(e) => return err_result(e),
        };

        let read = papers
            .iter()
            .filter(|p| p.reading_status == ReadingStatus::Completed)
            .count();
        let queued = papers
            .iter()
            .filter(|p| p.reading_status == ReadingStatus::Queued)
            .count();
        let rated = papers.iter().filter(|p| p.rating.is_some()).count();

        let per_topic: Vec<Value> = topics
            .iter()
            .map(|t| {
                let state = store.get_research_state(&t.id).ok().flatten();
                json!({
                    "name": t.name,
                    "state": state,
                })
            })
            .collect();

        ok_value(json!({
            "topics": topics.len(),
            "papers": papers.len(),
            "gaps": gaps.len(),
            "read": read,
            "queued": queued,
            "rated": rated,
            "per_topic": per_topic,
        }))
    }

    #[tool(
        description = "Update reading status and/or 1–5 rating of a paper. With neither set, returns the current paper."
    )]
    pub fn update_read(&self, Parameters(p): Parameters<UpdateReadParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };

        // Validate rating bounds before any side effect.
        let rating = match p.rating {
            Some(r) => Some(match Rating::new(r) {
                Ok(rating) => rating,
                Err(e) => return err_result(e),
            }),
            None => None,
        };

        if let Some(status) = &p.status
            && let Err(e) =
                store.update_reading_status(&p.id, ReadingStatus::from_str_lossy(status))
        {
            return err_result(e);
        }
        if let Some(rating) = rating
            && let Err(e) = store.update_rating(&p.id, rating)
        {
            return err_result(e);
        }

        if p.status.is_none() && rating.is_none() {
            // Lookup-only: an unknown id is an error, not a null result.
            return match store.get_paper(&p.id) {
                Ok(Some(paper)) => ok_value(serde_json::to_value(&paper).unwrap_or(Value::Null)),
                Ok(None) => err_result(ResearchError::NotFound(format!("paper '{}'", p.id))),
                Err(e) => err_result(e),
            };
        }
        ok_value(json!({ "updated": p.id, "status": p.status, "rating": p.rating }))
    }
}

#[tool_handler]
impl ServerHandler for ResearchServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        let mut info = rmcp::model::ServerInfo::default();
        info.server_info = rmcp::model::Implementation::new("research", env!("CARGO_PKG_VERSION"));
        // Hosts only load tools the server advertises; without this the
        // capabilities object serializes empty and every client sees 0 tools.
        info.capabilities = rmcp::model::ServerCapabilities::builder()
            .enable_tools()
            .build();
        info.instructions = Some(
            "research-agent: personal academic research memory. Drive the flow: \
             init, ingest, query_papers, analyze_gaps, then generate_report. \
             Organize with topics_list/topic_add, check the overview with state, \
             and track progress with update_read."
                .into(),
        );
        info
    }
}
