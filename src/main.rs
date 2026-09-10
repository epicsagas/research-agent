use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::path::PathBuf;

use research_agent::application::gap_analyzer::GapAnalyzer;
use research_agent::application::ingest_pipeline::IngestPipeline;
use research_agent::application::report_generator::ReportGenerator;
use research_agent::composition::{make_llm_engine, open_store, resolve_db};
use research_agent::config::{Config, default_config_path};
use research_agent::domain::paper::{Rating, ReadingStatus};
use research_agent::domain::research_topic::ResearchTopic;
use research_agent::ports::index_store::IndexStore;
use research_agent::ports::research_engine::ResearchEngine;

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
    Init {
        /// Skip the interactive onboarding (also automatic when stdin is not
        /// a terminal)
        #[arg(long)]
        no_onboard: bool,
    },

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

    /// Push paper tags into a running Zotero (dry-run by default; matched by
    /// DOI). CLI-only by design — never exposed as an MCP tool.
    Export {
        /// Export sink (only zotero today)
        #[arg(long, default_value = "zotero")]
        to: String,
        /// Actually write; without this, only a report is printed
        #[arg(long)]
        apply: bool,
    },

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

    /// Generate search keywords for papers (improves recall for queries whose
    /// wording differs from the abstract). Uses the configured `[llm]`
    /// provider; without one, prints the work queue for a host agent to fill
    /// via `research enrich <ID> --keywords "..."`.
    Enrich {
        /// Write keywords for this single paper (requires --keywords)
        id: Option<String>,
        /// Keywords to store, e.g. "transformer; self-attention". Mechanical
        /// write — no LLM call, so a host agent can supply them.
        #[arg(long)]
        keywords: Option<String>,
        /// Only papers linked to this topic
        #[arg(long)]
        topic: Option<String>,
        /// Include papers that already have keywords (re-generate them)
        #[arg(long)]
        force: bool,
        /// Maximum papers to process
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },

    /// Start the local web dashboard (127.0.0.1, read-only)
    Dashboard {
        /// Port override (the config file's `[dashboard] port` otherwise)
        #[arg(long)]
        port: Option<u16>,
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
async fn main() {
    init_tracing();
    if let Err(e) = run().await {
        // Display, not the `Termination` Debug path: anyhow's Debug prints a
        // captured backtrace (ONNX/ort symbols) at the user's terminal.
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let db = resolve_db(&cli.db);

    match cli.command {
        Commands::Init { no_onboard } => cmd_init(db, no_onboard)?,
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
        Commands::Reingest { missing_pages } => cmd_reingest(db, missing_pages).await?,
        Commands::Gaps { topic } => cmd_gaps(db, topic).await?,
        Commands::Report { title, topic } => cmd_report(db, title, topic).await?,
        Commands::Topics { action } => cmd_topics(db, action)?,
        Commands::Status => cmd_status(db)?,
        Commands::Export { to, apply } => cmd_export(db, &to, apply).await?,
        Commands::Read {
            id,
            status,
            rating,
            body,
        } => cmd_read(db, id, status, rating, body)?,
        Commands::Enrich {
            id,
            keywords,
            topic,
            force,
            limit,
        } => cmd_enrich(db, id, keywords, topic, force, limit).await?,
        Commands::Dashboard { port } => {
            let config_path = default_config_path();
            research_agent::dashboard::serve(cli.db.clone(), config_path, port).await?;
        }
        #[cfg(feature = "mcp")]
        Commands::Mcp => cmd_serve(db).await?,
    }

    Ok(())
}

fn cmd_init(db_path: PathBuf, no_onboard: bool) -> Result<()> {
    let config_path = default_config_path();
    // An existing config is never clobbered: onboarding offers its values as
    // defaults, and non-interactive runs leave the file byte-for-byte alone.
    let interactive = !no_onboard && std::io::stdin().is_terminal();
    if !interactive {
        if config_path.exists() {
            let config = research_agent::config::Config::load(&config_path)?;
            open_store(&config.database_path)?;
            println!("Existing config kept: {}", config_path.display());
            println!("Re-run `research init` in a terminal to reconfigure interactively.");
        } else {
            let config = Config {
                database_path: db_path,
                ..Config::default()
            };
            let db = config.database_path.clone();
            config.save(&config_path)?;
            open_store(&db)?;
            println!("Initialized research workspace (defaults; edit the config to configure).");
            println!("  Config: {}", config_path.display());
            println!("  DB:     {}", db.display());
        }
        return Ok(());
    }

    let existing = config_path
        .exists()
        .then(|| research_agent::config::Config::load(&config_path))
        .transpose()?;
    let config = research_agent::onboard::run(db_path, existing)?;
    config.save(&config_path)?;
    let store = open_store(&config.database_path)?;
    drop(store);
    println!("Initialized research workspace.");
    println!("  Config: {}", config_path.display());
    println!("  DB:     {}", config.database_path.display());
    if config.llm.is_none() {
        println!(
            "No [llm] configured — gaps/report return placeholders until set. Re-run `research init`."
        );
    }
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
                        if research_agent::application::identity::is_already_stored(&store, &paper)?
                        {
                            println!("Already ingested, skipped: {}", paper.title);
                            continue;
                        }
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

        let mut sources: Vec<Box<dyn research_agent::ports::paper_source::PaperSource>> =
            Vec::new();
        if source == "arxiv" || source == "all" {
            sources.push(Box::new(
                research_agent::adapters::arxiv_source::ArxivSource::new(),
            ));
        }
        if source == "s2" || source == "all" {
            sources.push(Box::new(
                research_agent::adapters::semantic_scholar_source::SemanticScholarSource::new(),
            ));
        }
        if source == "openalex" || source == "all" {
            sources.push(Box::new(
                research_agent::adapters::openalex_source::OpenAlexSource::new(),
            ));
        }
        if source == "europepmc" || source == "all" {
            sources.push(Box::new(
                research_agent::adapters::europepmc_source::EuropePmcSource::new(),
            ));
        }
        if source == "preprints" || source == "all" {
            sources.push(Box::new(
                research_agent::adapters::europepmc_source::PreprintSource::new(),
            ));
        }
        // Per-source failure is a warning, not an abort: partial success must
        // still reach the topic-linking step below.
        let refs: Vec<&dyn research_agent::ports::paper_source::PaperSource> =
            sources.iter().map(|s| s.as_ref()).collect();
        all_papers.extend(
            research_agent::application::ingest_pipeline::run_sources(&refs, &store, &q, limit)
                .await,
        );
    }

    if !all_papers.is_empty() {
        download_arxiv_bodies(&store, &all_papers, &pdf_download_dir(&db)).await?;
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
    if !all_papers.is_empty() {
        println!(
            "Run `research enrich` to add search keywords (improves recall for \
             queries worded differently from the abstract)."
        );
    }
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
        println!("Index rebuilt.");
    } else {
        println!("Index is auto-maintained. Use --rebuild to force.");
    }
    Ok(())
}

fn cmd_query(db: PathBuf, query: String, limit: usize, evidence: bool) -> Result<()> {
    let store = open_store(&db)?;
    let results = store.search_papers(&query, limit)?;

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

/// Where downloaded arXiv PDFs are cached: `pdf/` next to the database, so a
/// workspace stays self-contained and `reingest` can re-extract from disk.
fn pdf_download_dir(db: &std::path::Path) -> PathBuf {
    db.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("pdf")
}

/// Download arXiv PDFs for papers that carry an `arxiv_id` but no stored
/// body, extract their full text, and keep the PDF on disk. A failed download
/// warns and moves on — a metadata-only library is the normal state, not an
/// error. Papers that already have a body are skipped, so the call is
/// idempotent.
async fn download_arxiv_bodies(
    store: &dyn IndexStore,
    papers: &[research_agent::domain::paper::Paper],
    pdf_dir: &std::path::Path,
) -> Result<()> {
    use research_agent::adapters::arxiv_source::ArxivSource;
    use research_agent::adapters::pdf_source::PdfSource;

    let src = ArxivSource::new();
    let pdf = PdfSource::new();
    let mut fetched = 0usize;

    for paper in papers {
        let Some(arxiv_id) = paper.arxiv_id.as_deref() else {
            continue;
        };
        if store.get_paper_body(&paper.id)?.is_some() {
            continue;
        }
        let bytes = match src.download_pdf(arxiv_id).await {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Warning: PDF download failed for {arxiv_id}: {e}");
                continue;
            }
        };
        match pdf.extract_body(&bytes, arxiv_id) {
            Ok(Some(body)) => {
                std::fs::create_dir_all(pdf_dir)?;
                let path = pdf_dir.join(format!("{arxiv_id}.pdf"));
                std::fs::write(&path, &bytes)?;
                store.set_paper_pdf_path(
                    &paper.id,
                    &path
                        .canonicalize()
                        .unwrap_or_else(|_| path.clone())
                        .display()
                        .to_string(),
                )?;
                store.set_paper_body(&paper.id, &body)?;
                fetched += 1;
                // arXiv asks automated fetches to space themselves out.
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Ok(None) => eprintln!("Warning: no text extracted from {arxiv_id}"),
            Err(e) => eprintln!("Warning: {arxiv_id}: {e}"),
        }
    }
    if fetched > 0 {
        println!("Stored full text for {fetched} paper(s) from downloaded arXiv PDFs.");
    }
    Ok(())
}

/// Re-extract bodies for papers ingested from a local PDF. Idempotent:
/// `set_paper_body` replaces, so a re-run costs time and nothing else.
/// Papers without a local PDF but with an arXiv id get their PDF downloaded
/// and their body extracted for the first time.
async fn cmd_reingest(db: PathBuf, missing_pages: bool) -> Result<()> {
    use research_agent::adapters::pdf_source::{PAGE_MARKER_PREFIX, PdfSource};

    let store = open_store(&db)?;
    let src = PdfSource::new();
    let pdf_dir = pdf_download_dir(&db);
    let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);

    for paper in store.list_papers(None)? {
        let Some(pdf_path) = paper.pdf_path.as_deref() else {
            if paper.arxiv_id.is_some() {
                download_arxiv_bodies(&store, std::slice::from_ref(&paper), &pdf_dir).await?;
            }
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
        println!("Run `research index --rebuild` to reindex the updated bodies.");
    }
    Ok(())
}

async fn cmd_enrich(
    db: PathBuf,
    id: Option<String>,
    keywords: Option<String>,
    topic: Option<String>,
    force: bool,
    limit: usize,
) -> Result<()> {
    let store = open_store(&db)?;

    // Mechanical single-paper write: the path a host agent uses when this
    // install has no [llm] provider of its own.
    if let Some(paper_id) = id {
        let Some(kws) = keywords else {
            anyhow::bail!("`research enrich <ID>` needs --keywords \"kw1; kw2\"");
        };
        store.set_paper_keywords(&paper_id, &kws)?;
        println!("Keywords set for {paper_id}.");
        return Ok(());
    }
    if keywords.is_some() {
        anyhow::bail!("--keywords applies to a single paper: `research enrich <ID> --keywords ...`");
    }

    let candidates: Vec<_> = match (&topic, force) {
        (Some(tid), false) => store
            .list_papers_by_topic(tid, Some(limit * 4))?
            .into_iter()
            .filter(|p| p.keywords.is_empty())
            .take(limit)
            .collect(),
        (Some(tid), true) => store
            .list_papers_by_topic(tid, Some(limit))?
            .into_iter()
            .collect(),
        (None, false) => store.papers_missing_keywords(limit)?,
        (None, true) => store.list_papers(Some(limit))?,
    };

    if candidates.is_empty() {
        println!("Nothing to enrich (every paper in scope already has keywords).");
        return Ok(());
    }

    let inner_store = open_store(&db)?;
    let engine = make_llm_engine(inner_store)?;
    let pairs = engine.extract_keywords(&candidates).await?;

    // No [llm] configured: print the queue so the calling agent can generate
    // keywords itself and write them back. Not an error — this is mode B.
    if pairs.is_empty() {
        println!("{} paper(s) need keywords:\n", candidates.len());
        for paper in &candidates {
            let abstract_snippet: String = paper.abstract_text.chars().take(200).collect();
            println!("{}\n  {}\n  {}\n", paper.id, paper.title, abstract_snippet);
        }
        println!(
            "No [llm] provider configured. Generate 5-10 English keywords per paper \n\
             (synonyms, expanded acronyms, alternative phrasings) and store them with:\n\
             \n  research enrich <ID> --keywords \"kw1; kw2; kw3\"\n"
        );
        return Ok(());
    }

    let mut written = 0usize;
    let mut failed = 0usize;
    for (paper_id, kws) in &pairs {
        match store.set_paper_keywords(paper_id, kws) {
            Ok(()) => written += 1,
            // A hallucinated id is the model's error, not a reason to abort the
            // whole batch — the other papers still get their keywords.
            Err(e) => {
                eprintln!("Warning: could not set keywords for {paper_id}: {e}");
                failed += 1;
            }
        }
    }
    println!("Enriched {written} paper(s).");
    if failed > 0 {
        println!("{failed} paper(s) skipped (see warnings above).");
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

async fn cmd_export(db: PathBuf, to: &str, apply: bool) -> Result<()> {
    if to != "zotero" {
        return Err(anyhow::anyhow!(
            "unknown export sink '{to}' — only 'zotero' is supported"
        ));
    }
    let store = open_store(&db)?;
    let sink = research_agent::adapters::zotero_write::ZoteroWrite::new();
    let report =
        research_agent::application::zotero_export::export_tags_to_zotero(&store, &sink, apply)
            .await?;
    if apply {
        println!("Zotero tags pushed.");
    } else {
        println!("Dry run (pass --apply to write).");
    }
    println!("{}", report.summary());
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
