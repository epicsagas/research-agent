use crate::application::identity::is_already_stored;
use crate::domain::paper::Paper;
use crate::error::Result;
use crate::ports::index_store::IndexStore;
use crate::ports::paper_source::PaperSource;

pub struct IngestPipeline<'a> {
    source: &'a dyn PaperSource,
    store: &'a dyn IndexStore,
}

impl<'a> IngestPipeline<'a> {
    pub fn new(source: &'a dyn PaperSource, store: &'a dyn IndexStore) -> Self {
        Self { source, store }
    }

    /// Fetch from the source and store papers that are new to the library.
    /// Duplicate detection lives in `identity::is_already_stored`, shared
    /// with `paper_import::run_import` and the PDF branch — without it,
    /// re-running a source (a whole-library Zotero read, a repeated arXiv
    /// query) re-inserts the same papers as fresh rows, since each
    /// `Paper::new` carries a new id.
    pub async fn run(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let papers = self.source.fetch_papers(query, limit).await?;
        let mut new_papers = Vec::with_capacity(papers.len());
        for paper in &papers {
            if is_already_stored(self.store, paper)? {
                continue;
            }
            self.store.insert_paper(paper)?;
            new_papers.push(paper.clone());
        }
        Ok(new_papers)
    }
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

        let papers = pipeline.run("transformers", 2).await.unwrap();
        assert_eq!(papers.len(), 2);

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

        assert_eq!(pipeline.run("q", 10).await.unwrap().len(), 2);

        let second = pipeline.run("q", 10).await.unwrap();
        assert_eq!(second.len(), 0, "the DOI-less paper is caught by title");
        assert_eq!(store.list_papers(None).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ingest_respects_limit() {
        let source = FakeSource(vec![paper("a"), paper("b"), paper("c")]);
        let store = SqliteStore::open_in_memory().unwrap();
        let pipeline = IngestPipeline::new(&source, &store);

        assert_eq!(pipeline.run("q", 2).await.unwrap().len(), 2);
        assert_eq!(store.list_papers(None).unwrap().len(), 2);
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
        assert!(!papers.is_empty());
        assert_eq!(store.list_papers(None).unwrap().len(), papers.len());
    }
}
