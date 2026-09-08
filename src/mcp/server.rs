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
use crate::adapters::europepmc_source::{EuropePmcSource, PreprintSource};
use crate::adapters::openalex_source::OpenAlexSource;
use crate::adapters::pdf_source::PdfSource;
use crate::adapters::semantic_scholar_source::SemanticScholarSource;
use crate::adapters::sqlite_store::SqliteStore;
use crate::application::gap_analyzer::{collect_brief, record_gaps};
use crate::application::ingest_pipeline::IngestPipeline;
use crate::application::report_generator::{collect_material, save_report};
use crate::composition::{load_config, open_store};
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
            Ok((paper, body)) => {
                store.insert_paper(&paper).map_err(err_result)?;
                if let Some(body) = body {
                    store.set_paper_body(&paper.id, &body).map_err(err_result)?;
                }
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
    if p.source == "openalex" || p.source == "all" {
        let oa = OpenAlexSource::new();
        let fetched = IngestPipeline::new(&oa, store)
            .run(q, p.limit)
            .await
            .map_err(err_result)?;
        papers.extend(fetched);
    }
    if p.source == "europepmc" || p.source == "all" {
        let epmc = EuropePmcSource::new();
        let fetched = IngestPipeline::new(&epmc, store)
            .run(q, p.limit)
            .await
            .map_err(err_result)?;
        papers.extend(fetched);
    }
    if p.source == "preprints" || p.source == "all" {
        let pre = PreprintSource::new();
        let fetched = IngestPipeline::new(&pre, store)
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
        description = "Ingest papers from arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), bioRxiv-style preprints, or local PDFs. source: arxiv|s2|openalex|europepmc|preprints|all|pdf. For arxiv/s2/openalex/europepmc/preprints/all a query is required; for pdf a path (file or dir) is required. Optionally link ingested papers to a topic. Network-heavy for remote sources (async)."
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

    #[tool(description = "Force a full rebuild of the search index (FTS and the vector index).")]
    pub fn index_rebuild(&self) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        if let Err(e) = store.rebuild_index() {
            return err_result(e);
        }
        match load_config().ok().and_then(|cfg| {
            crate::application::hybrid_search::HybridSearch::open(
                &store,
                cfg.search.as_ref(),
                crate::application::hybrid_search::index_path_for(&self.ctx.db_path),
            )
        }) {
            Some(mut hybrid) => match hybrid.rebuild(&store) {
                Ok(()) => ok_value(json!({"rebuilt": true, "vector_index": true})),
                Err(e) => {
                    ok_value(json!({"rebuilt": true, "vector_index": false, "note": e.to_string()}))
                }
            },
            None => ok_value(json!({"rebuilt": true, "vector_index": false})),
        }
    }

    #[tool(
        description = "Import papers from BibTeX/BibLaTeX (.bib) or CSL-JSON (.json) files — e.g. a Zotero export. path is a file or a directory (all matching files are imported). Papers with a DOI already in the library are skipped."
    )]
    pub fn import_papers(&self, Parameters(p): Parameters<ImportPapersParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        match crate::application::paper_import::run_import(&store, std::path::Path::new(&p.path)) {
            Ok(summary) => ok_value(json!({
                "imported": summary.imported.len(),
                "skipped_duplicates": summary.skipped_duplicates,
                "failed": summary.failed,
                "papers": serde_json::to_value(&summary.imported).unwrap_or(Value::Null),
            })),
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "Fetch the stored full body text of a paper (from PDF ingest), section headings marked with '## '. Returns text, or has_body=false if only metadata is stored."
    )]
    pub fn paper_body(&self, Parameters(p): Parameters<PaperBodyParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        match store.get_paper_body(&p.id) {
            Ok(Some(body)) => {
                // Cap what flows into an agent's context window; the full text
                // stays in the DB (`research read <id> --body` prints it all).
                const MAX_TOOL_BODY_CHARS: usize = 40_000;
                let truncated = body.chars().take(MAX_TOOL_BODY_CHARS).collect::<String>();
                ok_value(json!({
                    "id": p.id,
                    "has_body": true,
                    "truncated": truncated.chars().count() < body.chars().count(),
                    "text": truncated,
                }))
            }
            Ok(None) => ok_value(json!({ "id": p.id, "has_body": false })),
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "Fetch and store citation-graph edges for a paper via OpenAlex, ingesting newly seen related papers into the library. direction \"references\" (default) lists the works the paper cites; \"cited_by\" lists the works citing it. Requires the paper to have an OpenAlex id or DOI. Idempotent. Network-heavy (async)."
    )]
    pub async fn paper_references(
        &self,
        Parameters(p): Parameters<PaperReferencesParams>,
    ) -> CallToolResult {
        let cited_by = match p.direction.as_deref() {
            None | Some("references") => false,
            Some("cited_by") => true,
            Some(other) => {
                return err_result(ResearchError::Source(format!(
                    "unknown direction '{other}'; use \"references\" or \"cited_by\""
                )));
            }
        };
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let paper = match store.get_paper(&p.id) {
            Ok(Some(paper)) => paper,
            Ok(None) => {
                return err_result(ResearchError::NotFound(format!(
                    "paper '{}' not found",
                    p.id
                )));
            }
            Err(e) => return err_result(e),
        };
        let synced = if cited_by {
            crate::application::references::sync_cited_by(&store, &paper).await
        } else {
            crate::application::references::sync_references(&store, &paper).await
        };
        match synced {
            Ok((papers, new_edges, new_papers)) => {
                let edges = if cited_by {
                    store.citations_citing_paper(&p.id)
                } else {
                    store.citations_for_paper(&p.id)
                };
                match edges {
                    Ok(edges) => ok_value(json!({
                        "id": p.id,
                        "direction": if cited_by { "cited_by" } else { "references" },
                        "edges": edges.len(),
                        "new_edges": new_edges,
                        "new_papers_count": new_papers,
                        // Resolved related papers (metadata only); already-known
                        // entries are included so the agent can see the full list.
                        "related_papers": papers,
                    })),
                    Err(e) => err_result(e),
                }
            }
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "Search the local paper index by query (hybrid lexical+semantic when the embedding backend is available, lexical otherwise). Returns matching papers (id, title, authors, year, status)."
    )]
    pub fn query_papers(&self, Parameters(p): Parameters<QueryPapersParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let hybrid = load_config().ok().and_then(|cfg| {
            crate::application::hybrid_search::HybridSearch::open(
                &store,
                cfg.search.as_ref(),
                crate::application::hybrid_search::index_path_for(&self.ctx.db_path),
            )
        });
        let results = match hybrid {
            Some(hybrid) => hybrid.search(&store, &p.query, p.limit),
            None => store.search_papers(&p.query, p.limit),
        };
        tool_result!(results)
    }

    #[tool(
        description = "Collect a topic brief: the topic, its papers with reading status, recorded gaps, and coverage state. Analyze this data yourself, then persist your findings with gaps_record."
    )]
    pub fn topic_brief(&self, Parameters(p): Parameters<TopicBriefParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        tool_result!(collect_brief(&store, &p.topic))
    }

    #[tool(
        description = "Record knowledge gaps you identified for a topic. Each gap is {description, gap_type?, priority?} where gap_type is one of missing_literature | unanswered_question | methodology_gap | connection_gap and priority is 0..=1."
    )]
    pub fn gaps_record(&self, Parameters(p): Parameters<GapsRecordParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let gaps: Vec<(String, Option<String>, Option<f32>)> = p
            .gaps
            .into_iter()
            .map(|g| (g.description, g.gap_type, g.priority))
            .collect();
        tool_result!(record_gaps(&store, &p.topic, &gaps))
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
        description = "Collect source material for a report: topic briefs (papers, gaps, coverage) for comma-separated topic ids. Draft the markdown yourself, then store it with report_save."
    )]
    pub fn report_material(&self, Parameters(p): Parameters<ReportTopicParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let topic_ids: Vec<String> = p.topics.split(',').map(|t| t.trim().to_string()).collect();
        tool_result!(collect_material(&store, &topic_ids))
    }

    #[tool(
        description = "Store an agent-authored markdown report. Sections are split on '## ' headings. Returns the report id and metadata."
    )]
    pub fn report_save(&self, Parameters(p): Parameters<ReportSaveParams>) -> CallToolResult {
        let store = match open_store(&self.ctx.db_path) {
            Ok(s) => s,
            Err(e) => return err_result(e),
        };
        let topic_ids: Vec<String> = p.topics.split(',').map(|t| t.trim().to_string()).collect();
        match save_report(&store, &p.title, &topic_ids, &p.markdown) {
            Ok(report) => ok_value(json!({
                "id": report.id,
                "title": report.title,
                "sections": report.sections.len(),
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
             init, ingest, query_papers, topic_brief, gaps_record, then report_material and report_save. \
             Organize with topics_list/topic_add, check the overview with state, \
             and track progress with update_read."
                .into(),
        );
        info
    }
}
