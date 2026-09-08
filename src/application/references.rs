use crate::adapters::openalex_source::ReferencesSource;
use crate::domain::citation::Citation;
use crate::domain::paper::Paper;
use crate::error::Result;
use crate::ports::index_store::IndexStore;

/// Fetch, persist, and return the OpenAlex references of `paper`. Idempotent:
/// referenced papers dedupe against library entries by OpenAlex id then DOI,
/// and citation edges dedupe on their paper-id pair. Returns the resolved
/// reference papers, how many edges were newly inserted, and how many papers
/// were newly ingested.
pub async fn sync_references(
    store: &dyn IndexStore,
    paper: &Paper,
) -> Result<(Vec<Paper>, usize, usize)> {
    let source = ReferencesSource::new();
    let work_ids = source.reference_ids(paper).await?;
    let hydrated = source.hydrate(&work_ids).await?;

    let (citations, new_papers) = link_related(store, &paper.id, &hydrated, false)?;
    let new_edges = store.insert_citations(&citations)?;
    Ok((hydrated, new_edges, new_papers))
}

/// Fetch, persist, and return the OpenAlex works citing `paper` (the reverse
/// direction of [`sync_references`]). Same dedupe/idempotency contract; the
/// edges point from each citing paper to `paper`.
pub async fn sync_cited_by(
    store: &dyn IndexStore,
    paper: &Paper,
) -> Result<(Vec<Paper>, usize, usize)> {
    let source = ReferencesSource::new();
    let citers = source.citing_papers(paper).await?;

    let (citations, new_papers) = link_related(store, &paper.id, &citers, true)?;
    let new_edges = store.insert_citations(&citations)?;
    Ok((citers, new_edges, new_papers))
}

/// Resolve `related` papers against the library (OpenAlex id, then DOI),
/// ingest unknown ones, and build citation edges between `paper` and each of
/// them. Forward produces `(paper -> related)` edges; reverse produces
/// `(related -> paper)`. Self-pairs add nothing to the graph and are skipped.
fn link_related(
    store: &dyn IndexStore,
    paper_id: &str,
    related: &[Paper],
    reverse: bool,
) -> Result<(Vec<Citation>, usize)> {
    let mut citations = Vec::with_capacity(related.len());
    let mut new_papers = 0usize;
    for other in related {
        let existing = match &other.openalex_id {
            Some(id) => store.find_paper_by_openalex_id(id)?,
            None => None,
        };
        let existing = match (&existing, &other.doi) {
            (None, Some(doi)) => store.find_paper_by_doi(doi)?,
            _ => existing,
        };
        let other_id = match existing {
            Some(p) => p.id,
            None => {
                store.insert_paper(other)?;
                new_papers += 1;
                other.id.clone()
            }
        };
        // Self-references exist in the wild (versioned preprints citing their
        // own journal article); storing `(a, a)` adds nothing to the graph.
        if other_id == paper_id {
            continue;
        }
        citations.push(if reverse {
            Citation::new(other_id, paper_id.to_string())
        } else {
            Citation::new(paper_id.to_string(), other_id)
        });
    }
    Ok((citations, new_papers))
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

        assert!(store.find_paper_by_openalex_id("W1").unwrap().is_some());
        assert!(store.find_paper_by_openalex_id("W2").unwrap().is_none());
    }

    /// Edge direction and self-pair skipping, without network. The
    /// self-pair case resolves through the store: the related work carries the
    /// anchor's OpenAlex id, so dedupe lands on the anchor itself.
    #[test]
    fn link_related_builds_directional_edges() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut paper = Paper::new("anchor".into());
        paper.openalex_id = Some("Wanchor".into());
        store.insert_paper(&paper).unwrap();

        let mut forward = Paper::new("forward ref".into());
        forward.openalex_id = Some("W1".into());
        let mut self_pair = Paper::new("same work, fresh record".into());
        self_pair.openalex_id = Some("Wanchor".into());
        let related = vec![forward.clone(), self_pair];

        let (citations, new_papers) = link_related(&store, &paper.id, &related, false).unwrap();
        assert_eq!(new_papers, 1, "only the forward ref is new");
        assert_eq!(citations.len(), 1, "self-pair skipped");
        assert_eq!(citations[0].citing_paper_id, paper.id);
        assert_eq!(citations[0].cited_paper_id, forward.id);

        let mut backward = Paper::new("backward citer".into());
        backward.openalex_id = Some("W2".into());
        let (citations, _) = link_related(&store, &paper.id, &[backward.clone()], true).unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].citing_paper_id, backward.id);
        assert_eq!(citations[0].cited_paper_id, paper.id);
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

        let (papers, new_edges, new_papers) = sync_references(&store, &paper).await.unwrap();
        assert!(!papers.is_empty());
        assert!(new_edges > 0);
        assert!(new_papers > 0);
        let edges = store.citations_for_paper(&paper.id).unwrap();
        // Edges track resolved references; fewer when a reference resolves to
        // the citing paper itself.
        assert!(edges.len() <= papers.len());
        // Every cited id resolves to an ingested paper.
        for edge in &edges {
            assert!(store.get_paper(&edge.cited_paper_id).unwrap().is_some());
        }
    }

    /// Live round-trip for the reverse direction, same contract as
    /// `live_sync_references`.
    #[tokio::test]
    #[ignore]
    async fn live_sync_cited_by() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut paper = Paper::new("Nanometre-scale thermometry in a living cell".into());
        paper.openalex_id = Some("W2741809807".into());
        store.insert_paper(&paper).unwrap();

        let (papers, new_edges, _new_papers) = sync_cited_by(&store, &paper).await.unwrap();
        assert!(!papers.is_empty());
        assert!(new_edges > 0);
        let edges = store.citations_citing_paper(&paper.id).unwrap();
        assert!(!edges.is_empty());
        // Every edge points at the anchor paper from an ingested citer.
        for edge in &edges {
            assert_eq!(edge.cited_paper_id, paper.id);
            assert!(store.get_paper(&edge.citing_paper_id).unwrap().is_some());
        }
    }
}
