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

    pub async fn run(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let papers = self.source.fetch_papers(query, limit).await?;
        for paper in &papers {
            self.store.insert_paper(paper)?;
        }
        Ok(papers)
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
