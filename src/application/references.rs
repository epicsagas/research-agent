use crate::adapters::openalex_source::ReferencesSource;
use crate::domain::citation::Citation;
use crate::domain::paper::Paper;
use crate::error::Result;
use crate::ports::index_store::IndexStore;

/// Fetch, persist, and return the OpenAlex references of `paper`. Idempotent:
/// referenced papers dedupe against library entries by OpenAlex id then DOI,
/// and citation edges dedupe on their paper-id pair.
pub async fn sync_references(
    store: &dyn IndexStore,
    paper: &Paper,
) -> Result<(Vec<Paper>, usize)> {
    let source = ReferencesSource::new();
    let work_ids = source.reference_ids(paper).await?;
    let hydrated = source.hydrate(&work_ids).await?;

    let mut citations = Vec::with_capacity(hydrated.len());
    for cited in &hydrated {
        let existing = match &cited.openalex_id {
            Some(id) => store.find_paper_by_openalex_id(id)?,
            None => None,
        };
        let existing = match (&existing, &cited.doi) {
            (None, Some(doi)) => store.find_paper_by_doi(doi)?,
            _ => existing,
        };
        let cited_id = match existing {
            Some(p) => p.id,
            None => {
                store.insert_paper(cited)?;
                cited.id.clone()
            }
        };
        citations.push(Citation::new(paper.id.clone(), cited_id));
    }

    let new_edges = store.insert_citations(&citations)?;
    Ok((hydrated, new_edges))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_store::SqliteStore;
    use crate::domain::paper::Paper;

    #[tokio::test]
    async fn unknown_identity_errors_before_any_write() {
        let store = SqliteStore::open_in_memory().unwrap();
        let paper = Paper::new("no ids".into());
        let err = sync_references(&store, &paper).await.unwrap_err();
        assert!(err.to_string().contains("no openalex_id or DOI"));
        assert!(store.list_papers(None).unwrap().is_empty());
    }

    /// Dedupe path, exercised without network: pre-store a paper by OpenAlex
    /// id, then verify the store helpers `sync_references` relies on behave.
    #[test]
    fn dedupes_referenced_paper_by_openalex_id() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut existing = Paper::new("already here".into());
        existing.openalex_id = Some("W1".into());
        store.insert_paper(&existing).unwrap();

        assert!(
            store
                .find_paper_by_openalex_id("W1")
                .unwrap()
                .is_some()
        );
        assert!(store.find_paper_by_openalex_id("W2").unwrap().is_none());
    }

    /// Live round-trip against the real OpenAlex API. Ignored by default: it
    /// needs network access. Run explicitly with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_sync_references() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut paper = Paper::new("Nanometre-scale thermometry in a living cell".into());
        paper.openalex_id = Some("W2741809807".into());
        store.insert_paper(&paper).unwrap();

        let (papers, new_edges) = sync_references(&store, &paper).await.unwrap();
        assert!(!papers.is_empty());
        assert!(new_edges > 0);
        let edges = store.citations_for_paper(&paper.id).unwrap();
        assert_eq!(edges.len(), papers.len());
        // Every cited id resolves to an ingested paper.
        for edge in &edges {
            assert!(store.get_paper(&edge.cited_paper_id).unwrap().is_some());
        }
    }
}
