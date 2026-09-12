//! The single place that decides whether a fetched or imported paper is
//! already in the library. Every ingest path routes through
//! [`is_already_stored`] — the DOI-only check used to be copy-pasted in the
//! ingest pipeline and the file importer and absent from the PDF branch,
//! which is how DOI-less papers came to duplicate on every run.

use crate::domain::paper::{Paper, normalize_title};
use crate::error::Result;
use crate::ports::index_store::IndexStore;

/// Identity key precedence: normalized DOI, then pdf_path, then normalized
/// title. The title key only fires when neither stronger key exists, so a
/// DOI-carrying paper that is new by DOI is not suppressed by an unrelated
/// same-titled row. Returns the stored row so callers can act on the existing
/// paper (link it to a topic, reuse its id) rather than only learning that a
/// duplicate exists.
pub fn find_already_stored(store: &dyn IndexStore, paper: &Paper) -> Result<Option<Paper>> {
    if let Some(doi) = paper.doi.as_deref()
        && let Some(found) = store.find_paper_by_doi(doi)?
    {
        return Ok(Some(found));
    }
    if let Some(path) = paper.pdf_path.as_deref()
        && let Some(found) = store.find_paper_by_pdf_path(path)?
    {
        return Ok(Some(found));
    }
    if paper.doi.is_none() && paper.pdf_path.is_none() {
        let title_key = normalize_title(&paper.title);
        if !title_key.is_empty() {
            return store.find_paper_by_title(&title_key);
        }
    }
    Ok(None)
}

pub fn is_already_stored(store: &dyn IndexStore, paper: &Paper) -> Result<bool> {
    Ok(find_already_stored(store, paper)?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_store::SqliteStore;

    fn paper(title: &str) -> Paper {
        Paper::new(title.to_string())
    }

    #[test]
    fn title_key_matches_ignoring_case_whitespace_and_punctuation() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut stored = paper("Attention Is All You Need");
        stored.id = "stored-1".into();
        store.insert_paper(&stored).unwrap();

        // Case, whitespace, and punctuation differences must not resurrect
        // the paper — BibTeX exports tend to end titles with a period.
        let again = paper("  attention   is all you need ");
        assert!(is_already_stored(&store, &again).unwrap());
        let punctuated = paper("Attention, Is All You Need!");
        assert!(is_already_stored(&store, &punctuated).unwrap());
    }

    #[test]
    fn title_key_does_not_fire_when_a_stronger_key_exists() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut stored = paper("Some Shared Title");
        stored.doi = Some("10.1/stored".into());
        store.insert_paper(&stored).unwrap();

        // Same title but a different DOI: the DOI decides, and it is new.
        let mut arriving = paper("Some Shared Title");
        arriving.doi = Some("10.1/arriving".into());
        assert!(!is_already_stored(&store, &arriving).unwrap());
    }

    #[test]
    fn pdf_path_key_matches_before_title_is_consulted() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut stored = paper("A Titled Paper");
        stored.pdf_path = Some("/library/papers/a.pdf".into());
        store.insert_paper(&stored).unwrap();

        let mut same_file = paper("A Titled Paper");
        same_file.pdf_path = Some("/library/papers/a.pdf".into());
        assert!(is_already_stored(&store, &same_file).unwrap());

        // Same path, different title: still the same file, still a duplicate.
        let mut retitled = paper("Retitled");
        retitled.pdf_path = Some("/library/papers/a.pdf".into());
        assert!(is_already_stored(&store, &retitled).unwrap());
    }

    #[test]
    fn genuinely_new_paper_is_not_a_duplicate() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_paper(&paper("known")).unwrap();
        assert!(!is_already_stored(&store, &paper("brand new")).unwrap());
    }

    /// Callers that repair state on a duplicate (topic linking) need the
    /// stored row itself, not just a bool — its id is the only one that exists
    /// in the database.
    #[test]
    fn find_already_stored_returns_the_stored_row() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut stored = paper("Known Paper");
        stored.id = "stored-7".into();
        store.insert_paper(&stored).unwrap();

        let found = find_already_stored(&store, &paper("known paper"))
            .unwrap()
            .expect("duplicate by title must resolve to the stored row");
        assert_eq!(found.id, "stored-7");
        assert!(
            find_already_stored(&store, &paper("other"))
                .unwrap()
                .is_none()
        );
    }
}
