use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use research_agent::application::gap_analyzer::GapAnalyzer;
use research_agent::application::ingest_pipeline::IngestPipeline;
use research_agent::application::report_generator::ReportGenerator;
use research_agent::composition::{load_config, make_llm_engine, open_store, resolve_db};
use research_agent::config::{Config, default_config_path};
use research_agent::domain::paper::{Rating, ReadingStatus};
use research_agent::domain::research_topic::ResearchTopic;
use research_agent::ports::index_store::IndexStore;

#[derive(Parser)]
#[command(name = "research", version, about = "Personal research assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database path (default: ~/.research/research.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize workspace
    Init,

    /// Import papers from BibTeX/BibLaTeX (.bib) or CSL-JSON (.json) files —
    /// e.g. a Zotero export. Directory paths import every matching file.
    Import {
        /// .bib / .bibtex / .json file(s) or directories
        paths: Vec<PathBuf>,
    },

    /// Ingest papers from external sources
    Ingest {
        /// Search query (not required for --source pdf; empty for --source
        /// zotero reads the whole library)
        query: Option<String>,

        /// Source: arxiv, s2, openalex, europepmc, preprints, all, pdf, or
        /// zotero (needs a running Zotero with the local API enabled)
        #[arg(long, default_value = "all")]
        source: String,

        /// Maximum papers to fetch (arxiv/s2/openalex/europepmc/preprints)
        #[arg(long, default_value_t = 10)]
        limit: usize,

        /// Path to PDF file or directory (required for --source pdf)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Link ingested papers to this topic ID
        #[arg(long)]
        topic: Option<String>,
    },

    /// Build or rebuild search index
    Index {
        /// Force full rebuild
        #[arg(long)]
        rebuild: bool,
    },

    /// Search papers
    Query {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Also show matching body text with its section and page
        #[arg(long)]
        evidence: bool,
    },

    /// Fetch and store the references of a paper (citation graph)
    References {
        /// Paper ID
        id: String,
        /// List the works citing this paper instead of its references
        #[arg(long)]
        cited_by: bool,
        /// Also label edges with Semantic Scholar citation intents
        /// (background/methodology/result, influential)
        #[arg(long)]
        intents: bool,
    },

    /// Re-extract stored PDF bodies (adds page markers to bodies ingested
    /// before they existed)
    Reingest {
        /// Only papers whose stored body has no page markers
        #[arg(long)]
        missing_pages: bool,
    },

    /// Analyze knowledge gaps
    Gaps {
        /// Topic ID to analyze
        #[arg(long)]
        topic: Option<String>,
    },

    /// Generate research report
    Report {
        /// Report title
        #[arg(long, default_value = "Research Report")]
        title: String,

        /// Topic IDs (comma-separated)
        #[arg(long)]
        topic: String,
    },

    /// Manage research topics
    Topics {
        #[command(subcommand)]
        action: TopicAction,
    },

    /// Show research state overview
    Status,

    /// Update reading status or rating of a paper
    Read {
        /// Paper ID
        id: String,

        /// New status: unread, queued, in_progress, completed, abandoned
        #[arg(long)]
        status: Option<String>,

        /// Rating 1–5
        #[arg(long)]
        rating: Option<u8>,

        /// Print the stored body text instead of updating anything
        #[arg(long)]
        body: bool,
    },

    /// Start the stdio MCP server (agent-driven mode; the primary interface for
    /// MCP hosts like Claude Code/Codex). Gated behind the `mcp` feature.
    #[cfg(feature = "mcp")]
    #[command(alias = "serve")]
    Mcp,
}

#[derive(Subcommand)]
enum TopicAction {
    /// List all topics
    List,

    /// Add a new topic
    Add {
        /// Topic name
        name: String,

        /// Optional description
        #[arg(long, default_value = "")]
        description: String,

        /// Parent topic ID (creates a sub-topic)
        #[arg(long)]
        parent: Option<String>,
    },
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "research=warn".into()),
        )
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let db = resolve_db(&cli.db);

    match cli.command {
        Commands::Init => cmd_init(db)?,
        Commands::Import { paths } => cmd_import(db, paths)?,
        Commands::Ingest {
            query,
            source,
            limit,
            path,
            topic,
        } => cmd_ingest(db, query, source, limit, path, topic).await?,
        Commands::Index { rebuild } => cmd_index(db, rebuild)?,
        Commands::Query {
            query,
            limit,
            evidence,
        } => cmd_query(db, query, limit, evidence)?,
        Commands::References {
            id,
            cited_by,
            intents,
        } => cmd_references(db, id, cited_by, intents).await?,
        Commands::Reingest { missing_pages } => cmd_reingest(db, missing_pages)?,
        Commands::Gaps { topic } => cmd_gaps(db, topic).await?,
        Commands::Report { title, topic } => cmd_report(db, title, topic).await?,
        Commands::Topics { action } => cmd_topics(db, action)?,
        Commands::Status => cmd_status(db)?,
        Commands::Read {
            id,
            status,
            rating,
            body,
        } => cmd_read(db, id, status, rating, body)?,
        #[cfg(feature = "mcp")]
        Commands::Mcp => cmd_serve(db).await?,
    }

    Ok(())
}

fn cmd_init(db_path: PathBuf) -> Result<()> {
    let config_path = default_config_path();
    let config = Config {
        database_path: db_path.clone(),
        llm: None,
        search: None,
    };
    config.save(&config_path)?;
    let store = open_store(&db_path)?;
    drop(store);
    println!("Initialized research workspace.");
    println!("  Config: {}", config_path.display());
    println!("  DB:     {}", db_path.display());
    println!("Edit {} to configure [llm].", config_path.display());
    Ok(())
}

async fn cmd_ingest(
    db: PathBuf,
    query: Option<String>,
    source: String,
    limit: usize,
    path: Option<PathBuf>,
    topic: Option<String>,
) -> Result<()> {
    let store = open_store(&db)?;
    let mut all_papers = Vec::new();

    if source == "pdf" {
        let pdf_path =
            path.ok_or_else(|| anyhow::anyhow!("--path <file|dir> is required for --source pdf"))?;
        let src = research_agent::adapters::pdf_source::PdfSource::new();
        let paths = research_agent::adapters::pdf_source::PdfSource::collect_paths(&pdf_path)?;
        if paths.is_empty() {
            println!("No PDF files found at {}", pdf_path.display());
        } else {
            for p in &paths {
                match src.ingest_file(p) {
                    Ok((paper, body)) => {
                        store.insert_paper(&paper)?;
                        if let Some(body) = body {
                            store.set_paper_body(&paper.id, &body)?;
                        }
                        println!("Ingested PDF: {}", paper.title);
                        all_papers.push(paper);
                    }
                    Err(e) => {
                        eprintln!("Warning: skipped {}: {e}", p.display());
                    }
                }
            }
        }
    } else if source == "zotero" {
        // A local library is read as-is, not discovered: an empty query means
        // the whole library, so the query stays optional here (and only here).
        let src = research_agent::adapters::zotero_source::ZoteroSource::new();
        let pipeline = IngestPipeline::new(&src, &store);
        let q = query.unwrap_or_default();
        let papers = pipeline.run(&q, limit).await?;
        println!("Ingested {} papers from Zotero", papers.len());
        all_papers.extend(papers);
    } else {
        let q = query
            .ok_or_else(|| anyhow::anyhow!("A search query is required for --source {source}"))?;

        if source == "arxiv" || source == "all" {
            let arxiv = research_agent::adapters::arxiv_source::ArxivSource::new();
            let pipeline = IngestPipeline::new(&arxiv, &store);
            let papers = pipeline.run(&q, limit).await?;
            println!("Ingested {} papers from arXiv", papers.len());
            all_papers.extend(papers);
        }

        if source == "s2" || source == "all" {
            let s2 =
                research_agent::adapters::semantic_scholar_source::SemanticScholarSource::new();
            let pipeline = IngestPipeline::new(&s2, &store);
            let papers = pipeline.run(&q, limit).await?;
            println!("Ingested {} papers from Semantic Scholar", papers.len());
            all_papers.extend(papers);
        }

        if source == "openalex" || source == "all" {
            let oa = research_agent::adapters::openalex_source::OpenAlexSource::new();
            let pipeline = IngestPipeline::new(&oa, &store);
            let papers = pipeline.run(&q, limit).await?;
            println!("Ingested {} papers from OpenAlex", papers.len());
            all_papers.extend(papers);
        }

        if source == "europepmc" || source == "all" {
            let epmc = research_agent::adapters::europepmc_source::EuropePmcSource::new();
            let pipeline = IngestPipeline::new(&epmc, &store);
            let papers = pipeline.run(&q, limit).await?;
            println!("Ingested {} papers from Europe PMC", papers.len());
            all_papers.extend(papers);
        }

        if source == "preprints" || source == "all" {
            let pre = research_agent::adapters::europepmc_source::PreprintSource::new();
            let pipeline = IngestPipeline::new(&pre, &store);
            let papers = pipeline.run(&q, limit).await?;
            println!("Ingested {} papers from preprint servers", papers.len());
            all_papers.extend(papers);
        }
    }

    if let Some(topic_id) = &topic {
        // The user explicitly asked to link to this topic — a missing
        // id is an error, not a warning to skip silently.
        if store.get_topic(topic_id)?.is_none() {
            anyhow::bail!("topic '{topic_id}' not found");
        }
        for paper in &all_papers {
            store.link_paper_to_topic(&paper.id, topic_id, paper.relevance_score)?;
        }
        println!("Linked {} papers to topic {topic_id}", all_papers.len());
    }

    println!("Total: {} papers ingested", all_papers.len());
    Ok(())
}

fn cmd_import(db: PathBuf, paths: Vec<PathBuf>) -> Result<()> {
    let store = open_store(&db)?;
    let mut total_imported = 0usize;
    let mut total_skipped = 0usize;
    let mut total_failed = 0usize;

    for path in &paths {
        let summary = research_agent::application::paper_import::run_import(&store, path)?;
        for paper in &summary.imported {
            println!("Imported: {}", paper.title);
        }
        for failure in &summary.failed {
            eprintln!("Warning: {failure}");
        }
        total_imported += summary.imported.len();
        total_skipped += summary.skipped_duplicates;
        total_failed += summary.failed.len();
    }

    println!(
        "Total: {total_imported} imported, {total_skipped} duplicate(s) skipped, {total_failed} file(s) failed"
    );
    Ok(())
}

fn cmd_index(db: PathBuf, rebuild: bool) -> Result<()> {
    let store = open_store(&db)?;
    if rebuild {
        store.rebuild_index()?;
        let hybrid = open_hybrid(&store, &db);
        if let Some(mut hybrid) = hybrid {
            hybrid.rebuild(&store)?;
        }
        println!("Index rebuilt.");
    } else {
        println!("Index is auto-maintained. Use --rebuild to force.");
    }
    Ok(())
}

/// Best-effort hybrid search stack: `None` (lexical-only fallback) when the
/// embedding backend or index is unavailable. Never surfaces errors.
fn open_hybrid(
    store: &research_agent::adapters::sqlite_store::SqliteStore,
    db_path: &std::path::Path,
) -> Option<research_agent::application::hybrid_search::HybridSearch> {
    let cfg = load_config().ok()?.search;
    research_agent::application::hybrid_search::HybridSearch::open(
        store,
        cfg.as_ref(),
        research_agent::application::hybrid_search::index_path_for(db_path),
    )
}

fn cmd_query(db: PathBuf, query: String, limit: usize, evidence: bool) -> Result<()> {
    let store = open_store(&db)?;
    let results = match open_hybrid(&store, &db) {
        Some(hybrid) => hybrid.search(&store, &query, limit)?,
        None => store.search_papers(&query, limit)?,
    };

    // Evidence is a separate pass over stored bodies: it answers "where in the
    // paper", which the ranked paper list cannot.
    if evidence {
        let hits = store.search_body_evidence(&query, None, limit)?;
        if hits.is_empty() {
            println!("No body-text matches for '{query}'.");
        } else {
            println!("Body matches:");
            for hit in &hits {
                let mut place = Vec::new();
                if let Some(section) = &hit.anchor.section {
                    place.push(section.clone());
                }
                if let Some(page) = hit.anchor.page {
                    place.push(format!("p.{page}"));
                }
                let place = if place.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", place.join(", "))
                };
                println!("- [{}] {}{}", hit.paper_id, hit.title, place);
                println!("    {}", hit.snippet.replace('\n', " "));
            }
            println!();
        }
    }
    if results.is_empty() {
        println!("No papers found for '{query}'.");
    } else {
        for paper in &results {
            let status = paper.reading_status.as_str();
            println!("[{}] {} ({})", paper.id, paper.title, status);
            if !paper.authors.is_empty() {
                println!("  Authors: {}", paper.authors.join(", "));
            }
            if let Some(year) = paper.year {
                println!("  Year: {year}");
            }
            println!();
        }
        println!("{} paper(s) found.", results.len());
    }
    Ok(())
}

async fn cmd_references(db: PathBuf, id: String, cited_by: bool, intents: bool) -> Result<()> {
    let store = open_store(&db)?;
    let paper = store
        .get_paper(&id)?
        .ok_or_else(|| anyhow::anyhow!("paper '{id}' not found"))?;

    // Fresh fetch; falls back to stored edges when the paper has no OpenAlex
    // identity or the network call fails, so "what should I read next" still
    // answers from earlier syncs.
    let (_papers, new_edges, new_papers) = match if cited_by {
        research_agent::application::references::sync_cited_by(&store, &paper).await
    } else {
        research_agent::application::references::sync_references(&store, &paper).await
    } {
        Ok((papers, new_edges, new_papers)) => (papers, new_edges, new_papers),
        Err(e) => {
            eprintln!("Warning: citation fetch failed ({e}); showing stored edges.");
            (Vec::new(), 0, 0)
        }
    };

    // Opt-in second pass: S2 labels only edges the graph already holds, and
    // costs its own rate-limited request, so it stays behind a flag.
    if intents {
        match research_agent::application::references::sync_citation_intents(
            &store, &paper, cited_by,
        )
        .await
        {
            Ok((labeled, unlabeled)) => {
                println!("Intents: {labeled} edge(s) labeled, {unlabeled} without a label.")
            }
            Err(e) => eprintln!("Warning: intent fetch failed ({e}); edges left unlabeled."),
        }
    }

    let label = if cited_by {
        "citing work(s)"
    } else {
        "reference(s)"
    };
    let edges = if cited_by {
        store.citations_citing_paper(&id)?
    } else {
        store.citations_for_paper(&id)?
    };
    if edges.is_empty() {
        println!("No {label} known for '{id}'.");
        return Ok(());
    }
    println!(
        "{} {label} ({} new), ingested {} new paper(s):",
        edges.len(),
        new_edges,
        new_papers
    );
    for edge in &edges {
        let other_id = if cited_by {
            &edge.citing_paper_id
        } else {
            &edge.cited_paper_id
        };
        let title = store
            .get_paper(other_id)?
            .map(|p| p.title)
            .unwrap_or_else(|| "(paper not in library)".into());
        if edge.context.is_empty() {
            println!("- [{other_id}] {title}");
        } else {
            println!("- [{other_id}] {title} ({})", edge.context);
        }
    }
    Ok(())
}

/// Re-extract bodies for papers ingested from a local PDF. Idempotent:
/// `set_paper_body` replaces, so a re-run costs time and nothing else.
fn cmd_reingest(db: PathBuf, missing_pages: bool) -> Result<()> {
    use research_agent::adapters::pdf_source::{PAGE_MARKER_PREFIX, PdfSource};

    let store = open_store(&db)?;
    let src = PdfSource::new();
    let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);

    for paper in store.list_papers(None)? {
        let Some(pdf_path) = paper.pdf_path.as_deref() else {
            continue;
        };
        if missing_pages {
            // Already re-extracted, or ingested after markers shipped.
            let has_markers = store
                .get_paper_body(&paper.id)?
                .is_some_and(|b| b.contains(PAGE_MARKER_PREFIX));
            if has_markers {
                skipped += 1;
                continue;
            }
        }
        let path = std::path::Path::new(pdf_path);
        if !path.exists() {
            // pdf_path is absolute and canonicalized at ingest, so a moved or
            // deleted file is expected rather than exceptional.
            eprintln!("Skipped (PDF missing): {} — {pdf_path}", paper.title);
            skipped += 1;
            continue;
        }
        match src.ingest_file(path) {
            Ok((_, Some(body))) => {
                store.set_paper_body(&paper.id, &body)?;
                done += 1;
            }
            Ok((_, None)) => {
                eprintln!("Skipped (no text extracted): {}", paper.title);
                skipped += 1;
            }
            Err(e) => {
                eprintln!("Failed: {} — {e}", paper.title);
                failed += 1;
            }
        }
    }

    println!("Re-ingested {done} paper(s), skipped {skipped}, failed {failed}.");
    if done > 0 {
        println!("Run `research index --rebuild` to re-embed the updated bodies.");
    }
    Ok(())
}

async fn cmd_gaps(db: PathBuf, topic: Option<String>) -> Result<()> {
    let store = open_store(&db)?;
    let inner_store = open_store(&db)?;
    let engine = make_llm_engine(inner_store)?;
    let analyzer = GapAnalyzer::new(&engine, &store);

    match topic {
        Some(tid) => {
            let gaps = analyzer.analyze(&tid).await?;
            if gaps.is_empty() {
                println!("No gaps found for topic {tid}.");
            } else {
                for gap in &gaps {
                    println!(
                        "[{}] {} ({})",
                        gap.id,
                        gap.description,
                        gap.gap_type.as_str()
                    );
                }
                println!("{} gap(s) identified.", gaps.len());
            }
        }
        None => {
            let gaps = store.list_gaps(None)?;
            if gaps.is_empty() {
                println!("No gaps recorded. Use --topic <ID> to analyze.");
            } else {
                for gap in &gaps {
                    println!(
                        "[{}] {} ({})",
                        gap.id,
                        gap.description,
                        gap.gap_type.as_str()
                    );
                }
                println!("{} gap(s) total.", gaps.len());
            }
        }
    }
    Ok(())
}

async fn cmd_report(db: PathBuf, title: String, topic: String) -> Result<()> {
    let store = open_store(&db)?;
    let inner_store = open_store(&db)?;
    let engine = make_llm_engine(inner_store)?;
    let generator = ReportGenerator::new(&engine, &store);

    let topic_ids: Vec<String> = topic.split(',').map(String::from).collect();
    let report = generator.generate(&title, &topic_ids).await?;
    println!("{}", report.to_markdown());
    println!("Report saved: {}", report.id);
    Ok(())
}

fn cmd_topics(db: PathBuf, action: TopicAction) -> Result<()> {
    let store = open_store(&db)?;
    match action {
        TopicAction::List => {
            let topics = store.list_topics()?;
            if topics.is_empty() {
                println!("No topics. Use `research topics add <NAME>` to create one.");
            } else {
                for t in &topics {
                    let indent = "  ".repeat(t.depth as usize);
                    println!("{}[{}] {} (depth {})", indent, t.id, t.name, t.depth);
                    if !t.description.is_empty() {
                        println!("{}  {}", indent, t.description);
                    }
                }
                println!("{} topic(s).", topics.len());
            }
        }
        TopicAction::Add {
            name,
            description,
            parent,
        } => {
            let mut topic = match parent {
                Some(parent_id) => match store.get_topic(&parent_id)? {
                    Some(parent_topic) => ResearchTopic::new_subtopic(name, &parent_topic),
                    None => {
                        anyhow::bail!("Parent topic '{parent_id}' not found");
                    }
                },
                None => ResearchTopic::new(name),
            };
            topic.description = description;

            let id = topic.id.clone();
            store.insert_topic(&topic)?;
            println!("Created topic: {id} (depth: {})", topic.depth);
        }
    }
    Ok(())
}

fn cmd_status(db: PathBuf) -> Result<()> {
    let store = open_store(&db)?;
    let topics = store.list_topics()?;
    let papers = store.list_papers(None)?;
    let gaps = store.list_gaps(None)?;

    println!("Research Status");
    println!("  Topics:  {}", topics.len());
    println!("  Papers:  {}", papers.len());
    println!("  Gaps:    {}", gaps.len());

    let read = papers
        .iter()
        .filter(|p| p.reading_status == ReadingStatus::Completed)
        .count();
    let queued = papers
        .iter()
        .filter(|p| p.reading_status == ReadingStatus::Queued)
        .count();
    let rated = papers.iter().filter(|p| p.rating.is_some()).count();
    println!("  Read:    {read}");
    println!("  Queued:  {queued}");
    println!("  Rated:   {rated}");

    if !topics.is_empty() {
        println!();
        for t in &topics {
            let state = store.get_research_state(&t.id)?;
            match state {
                Some(s) => {
                    println!(
                        "  [{}] papers_read={}, gaps={}, coverage={:.0}%",
                        t.name,
                        s.papers_read,
                        s.gaps_identified,
                        s.coverage_score * 100.0
                    );
                }
                None => {
                    println!("  [{}] no state yet", t.name);
                }
            }
        }
    }
    Ok(())
}

fn cmd_read(
    db: PathBuf,
    id: String,
    status: Option<String>,
    rating: Option<u8>,
    body: bool,
) -> Result<()> {
    let store = open_store(&db)?;
    let mut updated = false;

    // Validate all inputs before any side effect so a bad rating does
    // not leave a reading-status update already committed.
    let rating = match rating {
        Some(r) => Some(Rating::new(r)?),
        None => None,
    };

    if body {
        if status.is_some() || rating.is_some() {
            anyhow::bail!("--body cannot be combined with --status/--rating");
        }
        return match store.get_paper_body(&id)? {
            Some(body) => {
                println!("{body}");
                Ok(())
            }
            None => {
                println!("No stored body for paper {id}.");
                Ok(())
            }
        };
    }

    if let Some(s) = status {
        let rs = ReadingStatus::from_str_lossy(&s);
        let status_str = rs.as_str().to_string();
        store.update_reading_status(&id, rs)?;
        println!("Updated paper {id} reading status to {status_str}");
        updated = true;
    }

    if let Some(rating) = rating {
        store.update_rating(&id, rating)?;
        println!("Updated paper {id} rating to {}/5", rating.get());
        updated = true;
    }

    if !updated {
        if let Some(paper) = store.get_paper(&id)? {
            println!("[{}] {}", paper.id, paper.title);
            println!("  Status:         {}", paper.status.as_str());
            println!("  Reading status: {}", paper.reading_status.as_str());
            if let Some(rating) = paper.rating {
                println!("  Rating:         {}/5", rating.get());
            }
            if !paper.authors.is_empty() {
                println!("  Authors: {}", paper.authors.join(", "));
            }
            if !paper.abstract_text.is_empty() {
                println!("  Abstract: {}", paper.abstract_text);
            }
        } else {
            println!("Paper {id} not found.");
        }
    }
    Ok(())
}

/// Start the research stdio MCP server. The DB path is fixed here and shared
/// (immutable) inside the server via `Arc`. The `research` binary then speaks
/// JSON-RPC 2.0 over stdio so an MCP host (Claude Code/Codex) can drive it.
#[cfg(feature = "mcp")]
async fn cmd_serve(db_path: PathBuf) -> Result<()> {
    let ctx = research_agent::mcp::server::ResearchContext {
        db_path: db_path.to_path_buf(),
    };
    research_agent::mcp::server::ResearchServer::new(ctx)
        .serve_stdio()
        .await
        .map_err(anyhow::Error::msg)
}
