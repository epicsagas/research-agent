use crate::application::identity::find_already_stored;
use crate::domain::paper::Paper;
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::paper_source::PaperSource;

/// What one ingest run left in the library. `new` holds the papers inserted
/// this run; `fetched` holds every paper from this fetch that is in the
/// library afterwards — new plus already-stored, carrying their stored ids.
/// Downstream passes (topic linking, body download) run over `fetched`: both
/// are idempotent, so covering already-stored papers is what lets a re-run
/// repair linking that an earlier crashed run missed.
#[derive(Default)]
pub struct IngestOutcome {
    pub new: Vec<Paper>,
    pub fetched: Vec<Paper>,
}

pub struct IngestPipeline<'a> {
    source: &'a dyn PaperSource,
    store: &'a dyn IndexStore,
}

impl<'a> IngestPipeline<'a> {
    pub fn new(source: &'a dyn PaperSource, store: &'a dyn IndexStore) -> Self {
        Self { source, store }
    }

    /// Fetch from the source and store papers that are new to the library.
    /// Duplicate detection lives in `identity::find_already_stored`, shared
    /// with `paper_import::run_import` and the PDF branch — without it,
    /// re-running a source (a whole-library Zotero read, a repeated arXiv
    /// query) re-inserts the same papers as fresh rows, since each
    /// `Paper::new` carries a new id.
    pub async fn run(&self, query: &str, limit: usize) -> Result<IngestOutcome> {
        let papers = self.source.fetch_papers(query, limit).await?;
        let mut outcome = IngestOutcome::default();
        for paper in &papers {
            match find_already_stored(self.store, paper)? {
                Some(stored) => outcome.fetched.push(stored),
                None => {
                    self.store.insert_paper(paper)?;
                    outcome.new.push(paper.clone());
                    outcome.fetched.push(paper.clone());
                }
            }
        }
        Ok(outcome)
    }
}

/// Ingest from several sources, tolerating per-source failure. One source
/// erroring (Semantic Scholar rate-limiting with HTTP 429, a network hiccup)
/// must not abort the run: papers already ingested from other sources still
/// need to reach the caller's topic-linking step. A failing source is
/// reported on stderr and the remaining sources continue.
pub async fn run_sources(
    sources: &[&dyn PaperSource],
    store: &dyn IndexStore,
    query: &str,
    limit: usize,
) -> IngestOutcome {
    let mut merged = IngestOutcome::default();
    for src in sources {
        match IngestPipeline::new(*src, store).run(query, limit).await {
            Ok(outcome) => {
                let stored = outcome.fetched.len() - outcome.new.len();
                println!(
                    "Ingested {} papers from {}{}",
                    outcome.new.len(),
                    src.name(),
                    if stored > 0 {
                        format!(" ({stored} already stored)")
                    } else {
                        String::new()
                    }
                );
                merged.new.extend(outcome.new);
                merged.fetched.extend(outcome.fetched);
            }
            Err(e) => eprintln!("Warning: {} ingest failed, skipping: {e}", src.name()),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::arxiv_source::ArxivSource;
    use crate::adapters::sqlite_store::SqliteStore;
    use crate::domain::paper::Paper;

    /// Canned source — the pipeline under test is persistence, not arXiv.
    struct FakeSource(Vec<Paper>);

    #[async_trait::async_trait]
    impl PaperSource for FakeSource {
        async fn fetch_papers(&self, _query: &str, limit: usize) -> Result<Vec<Paper>> {
            Ok(self.0.iter().take(limit).cloned().collect())
        }

        fn name(&self) -> &str {
            "fake"
        }
    }

    fn paper(title: &str) -> Paper {
        Paper::new(title.to_string())
    }

    fn paper_with_doi(title: &str, doi: &str) -> Paper {
        let mut p = Paper::new(title.to_string());
        p.doi = Some(doi.to_string());
        p
    }

    #[tokio::test]
    async fn ingest_stores_papers() {
        let source = FakeSource(vec![paper("attention"), paper("scaling laws")]);
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        let outcome = pipeline.run("transformers", 2).await.unwrap();
        assert_eq!(outcome.new.len(), 2);
        assert_eq!(outcome.fetched.len(), 2);

        let stored = store.list_papers(None).unwrap();
        assert_eq!(stored.len(), 2);
    }

    /// Re-running a source must not duplicate anything — a whole-library
    /// Zotero re-read would otherwise re-insert every row, and DOI-less
    /// items (metadata fetches fail on real libraries) used to duplicate
    /// even after the DOI check existed.
    #[tokio::test]
    async fn ingest_skips_papers_already_stored_by_doi_or_title() {
        let source = FakeSource(vec![
            paper_with_doi("known paper", "10.1/known"),
            paper("no-doi paper"),
        ]);
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        assert_eq!(pipeline.run("q", 10).await.unwrap().new.len(), 2);

        let second = pipeline.run("q", 10).await.unwrap();
        assert_eq!(second.new.len(), 0, "the DOI-less paper is caught by title");
        assert_eq!(
            second.fetched.len(),
            2,
            "stored rows still reach downstream passes"
        );
        assert_eq!(store.list_papers(None).unwrap().len(), 2);
    }

    /// A re-run must return the STORED rows (their real ids), not the freshly
    /// fetched copies (whose ids are not in the database). Topic linking on a
    /// re-run links through these ids — fresh ids would link nothing.
    #[tokio::test]
    async fn rerun_returns_stored_rows_with_their_real_ids() {
        let source = FakeSource(vec![paper("known paper")]);
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        pipeline.run("q", 10).await.unwrap();
        let stored = store.list_papers(None).unwrap();
        let stored_ids: Vec<String> = stored.iter().map(|p| p.id.clone()).collect();

        let second = pipeline.run("q", 10).await.unwrap();
        let second_ids: Vec<String> = second.fetched.iter().map(|p| p.id.clone()).collect();
        assert_eq!(second_ids, stored_ids);
    }

    #[tokio::test]
    async fn ingest_respects_limit() {
        let source = FakeSource(vec![paper("a"), paper("b"), paper("c")]);
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        assert_eq!(pipeline.run("q", 2).await.unwrap().new.len(), 2);
        assert_eq!(store.list_papers(None).unwrap().len(), 2);
    }

    /// Regression: `--source all` used to abort on the first failing source
    /// (e.g. Semantic Scholar 429), so papers already ingested from arXiv
    /// never reached topic linking. A failed source is skipped; the rest run.
    #[tokio::test]
    async fn run_sources_survives_partial_failure() {
        struct FailingSource;

        #[async_trait::async_trait]
        impl PaperSource for FailingSource {
            async fn fetch_papers(&self, _query: &str, _limit: usize) -> Result<Vec<Paper>> {
                Err(crate::error::ResearchError::Source(
                    "rate limited (429)".into(),
                ))
            }
            fn name(&self) -> &str {
                "failing"
            }
        }

        let store = SqliteStore::open_in_memory().unwrap();
        let ok = FakeSource(vec![paper("kept paper")]);
        let failing = FailingSource;
        let sources: [&dyn PaperSource; 2] = [&failing, &ok];

        let got = run_sources(&sources, &store, "q", 10).await;

        assert_eq!(got.fetched.len(), 1, "the healthy source's papers are kept");
        assert_eq!(store.list_papers(None).unwrap().len(), 1);
    }

    /// Live round-trip against the real arXiv API. Ignored by default: it
    /// needs network access and fails whenever arXiv rate-limits (HTTP 429).
    /// Run explicitly with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "hits the live arXiv API (network + rate limits)"]
    async fn ingest_fetches_live_arxiv() {
        let source = ArxivSource::new();
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        let papers = pipeline.run("transformers", 2).await.unwrap();
        assert!(!papers.new.is_empty());
        assert_eq!(store.list_papers(None).unwrap().len(), papers.fetched.len());
    }
}
