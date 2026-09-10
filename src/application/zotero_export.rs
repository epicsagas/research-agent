//! Push library tags into a running Zotero: the narrow write slice from the
//! roadmap. Papers are matched to Zotero items by normalized DOI; papers
//! without a DOI, without a Zotero counterpart, or whose Zotero item changed
//! since the read are skipped and counted, never guessed at. Dry-run is the
//! default — the caller opts into writes with an explicit flag, and nothing
//! here is exposed over MCP (a CLI user pressing the button is the trust
//! boundary).

use async_trait::async_trait;

use crate::adapters::bib_importer::normalize_doi;
use crate::adapters::zotero_write::{RemoteItem, merge_tags};
use crate::error::Result;
use crate::ports::index_store::IndexStore;

/// The write half's storage contract, split from the HTTP adapter so the
/// dry-run/conflict logic is testable offline.
#[async_trait]
pub trait ZoteroTagSink: Send + Sync {
    async fn library(&self) -> Result<Vec<RemoteItem>>;
    /// `Ok(false)` = version conflict, item changed in Zotero: skip, report.
    async fn write_tags(&self, key: &str, version: i64, tags: &[String]) -> Result<bool>;
}

/// What one export pass did (or would do, in dry-run). Every skip reason is
/// counted, not collapsed into a silent total.
#[derive(Debug, Default, PartialEq)]
pub struct ExportReport {
    /// Papers whose tags were written (`apply`) — or would be written
    /// (dry-run) — as `title: added tags`.
    pub updates: Vec<String>,
    /// Papers with no tags to add whose Zotero item already matches.
    pub unchanged: usize,
    pub skipped_no_doi: usize,
    pub skipped_not_in_zotero: usize,
    /// Several Zotero items share the paper's DOI; picking one would be a
    /// guess, so all of them are left alone.
    pub skipped_ambiguous: usize,
    /// Version conflict: the Zotero item changed since the read. Never
    /// merged; the user resolves it in Zotero.
    pub skipped_conflict: usize,
}

impl ExportReport {
    /// One human line for the CLI: totals first, then up to a few updates.
    pub fn summary(&self) -> String {
        let mut line = format!(
            "{} matched, {} unchanged, {} without DOI, {} not in Zotero, {} ambiguous DOI, {} changed in Zotero",
            self.updates.len(),
            self.unchanged,
            self.skipped_no_doi,
            self.skipped_not_in_zotero,
            self.skipped_ambiguous,
            self.skipped_conflict,
        );
        if let Some(first) = self.updates.first() {
            line.push_str(&format!("\n  e.g. {first}"));
            if self.updates.len() > 1 {
                line.push_str(&format!("\n  ... and {} more", self.updates.len() - 1));
            }
        }
        line
    }
}

/// Match every library paper carrying a DOI to its Zotero item and push the
/// union of the two tag sets. With `apply = false` nothing is written: the
/// sink is never called and the report describes what *would* happen.
pub async fn export_tags_to_zotero(
    store: &dyn IndexStore,
    sink: &dyn ZoteroTagSink,
    apply: bool,
) -> Result<ExportReport> {
    let remote = sink.library().await?;
    // Normalize both sides at match time, so the exporter never depends on
    // the sink (or the import path) having normalized already. Items sharing
    // a DOI are collected: a match must be unambiguous or it is skipped.
    let mut by_doi: std::collections::HashMap<String, Vec<&RemoteItem>> =
        std::collections::HashMap::new();
    for item in &remote {
        if let Some(doi) = item.doi.as_deref().and_then(normalize_doi) {
            by_doi.entry(doi).or_default().push(item);
        }
    }

    let mut report = ExportReport::default();
    // DOIs already written this pass: after a successful write the cached
    // item version is stale, so a second library paper with the same DOI
    // would turn into a spurious conflict.
    let mut written: std::collections::HashSet<String> = std::collections::HashSet::new();
    for paper in store.list_papers(None)? {
        // Stored DOIs are normalized at import; normalizing again is free and
        // keeps matching exact even for rows that predate that guarantee.
        let Some(doi) = paper.doi.as_deref().and_then(normalize_doi) else {
            report.skipped_no_doi += 1;
            continue;
        };
        let Some(items) = by_doi.get(&doi) else {
            report.skipped_not_in_zotero += 1;
            continue;
        };
        let &[item] = items.as_slice() else {
            report.skipped_ambiguous += 1;
            continue;
        };
        // Compare against the item's tags the way merge would store them:
        // padding-only differences are not a change worth a write.
        let current: Vec<String> = item
            .tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let merged = merge_tags(&current, &paper.tags);
        if merged == current {
            report.unchanged += 1;
            continue;
        }
        if written.contains(&doi) {
            // Already pushed this pass; the tag set in Zotero now includes
            // everything this paper would add.
            report.unchanged += 1;
            continue;
        }
        if !apply {
            report.updates.push(format!(
                "{}: +{} tag(s)",
                paper.title,
                merged.len() - item.tags.len()
            ));
            continue;
        }
        if sink.write_tags(&item.key, item.version, &merged).await? {
            written.insert(doi);
            report.updates.push(format!(
                "{}: +{} tag(s)",
                paper.title,
                merged.len() - item.tags.len()
            ));
        } else {
            report.skipped_conflict += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::paper::Paper;
    use std::sync::Mutex;

    struct FakeSink {
        items: Vec<RemoteItem>,
        /// (key, tags) of every write attempt.
        writes: Mutex<Vec<(String, Vec<String>)>>,
        /// Keys that answer with a version conflict.
        conflicts: Vec<String>,
    }

    impl FakeSink {
        fn new(items: Vec<RemoteItem>) -> Self {
            Self {
                items,
                writes: Mutex::new(Vec::new()),
                conflicts: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl ZoteroTagSink for FakeSink {
        async fn library(&self) -> Result<Vec<RemoteItem>> {
            Ok(self.items.clone())
        }
        async fn write_tags(&self, key: &str, _version: i64, tags: &[String]) -> Result<bool> {
            self.writes
                .lock()
                .unwrap()
                .push((key.to_string(), tags.to_vec()));
            Ok(!self.conflicts.iter().any(|c| c == key))
        }
    }

    fn remote(key: &str, doi: &str, tags: &[&str]) -> RemoteItem {
        RemoteItem {
            key: key.into(),
            version: 7,
            doi: Some(doi.into()),
            tags: tags.iter().map(|t| (*t).into()).collect(),
        }
    }

    fn paper(title: &str, doi: &str, tags: &[&str]) -> Paper {
        let mut p = Paper::new(title.into());
        p.doi = Some(doi.into());
        p.tags = tags.iter().map(|t| (*t).into()).collect();
        p
    }

    async fn run(papers: Vec<Paper>, sink: &FakeSink, apply: bool) -> ExportReport {
        let dir = tempfile::tempdir().unwrap();
        let store =
            crate::adapters::sqlite_store::SqliteStore::open(&dir.path().join("t.db")).unwrap();
        for p in &papers {
            store.insert_paper(p).unwrap();
        }
        export_tags_to_zotero(&store, sink, apply).await.unwrap()
    }

    #[tokio::test]
    async fn dry_run_reports_without_writing() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &["zotero-tag"])]);
        let report = run(
            vec![paper("P", "10.1/a", &["zotero-tag", "mine"])],
            &sink,
            false,
        )
        .await;
        assert_eq!(report.updates.len(), 1);
        assert!(
            sink.writes.lock().unwrap().is_empty(),
            "dry-run must not write"
        );
    }

    #[tokio::test]
    async fn apply_merges_and_writes_union() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &["zotero-tag"])]);
        let report = run(vec![paper("P", "10.1/a", &["mine"])], &sink, true).await;
        assert_eq!(report.updates.len(), 1);
        let writes = sink.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "K1");
        assert_eq!(writes[0].1, vec!["zotero-tag", "mine"]);
    }

    #[tokio::test]
    async fn doi_matching_ignores_case_and_resolver_prefix() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/A", &[])]);
        let report = run(
            vec![paper("P", "https://doi.org/10.1/a", &["mine"])],
            &sink,
            true,
        )
        .await;
        assert_eq!(report.updates.len(), 1, "normalized DOIs match");
    }

    #[tokio::test]
    async fn unchanged_items_are_counted_not_written() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &["same"])]);
        let report = run(vec![paper("P", "10.1/a", &["same"])], &sink, true).await;
        assert_eq!(report.unchanged, 1);
        assert!(report.updates.is_empty());
        assert!(sink.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn skips_are_counted_by_reason() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &[])]);
        let report = run(
            vec![
                paper("has doi but absent", "10.1/missing", &[]),
                paper("no doi at all", "", &[]),
            ],
            &sink,
            false,
        )
        .await;
        assert_eq!(report.skipped_not_in_zotero, 1);
        assert_eq!(report.skipped_no_doi, 1);
    }

    #[tokio::test]
    async fn version_conflicts_skip_and_count() {
        let mut sink = FakeSink::new(vec![remote("K1", "10.1/a", &["old"])]);
        sink.conflicts.push("K1".into());
        let report = run(vec![paper("P", "10.1/a", &["mine"])], &sink, true).await;
        assert_eq!(report.skipped_conflict, 1);
        assert!(report.updates.is_empty());
    }

    #[test]
    fn summary_names_every_bucket() {
        let report = ExportReport {
            updates: vec!["T: +1 tag(s)".into()],
            unchanged: 2,
            skipped_no_doi: 3,
            skipped_not_in_zotero: 4,
            skipped_ambiguous: 6,
            skipped_conflict: 5,
        };
        let s = report.summary();
        for needle in [
            "1 matched",
            "2 unchanged",
            "3 without DOI",
            "4 not in Zotero",
            "6 ambiguous DOI",
            "5 changed in Zotero",
        ] {
            assert!(s.contains(needle), "summary missing {needle}: {s}");
        }
    }

    #[tokio::test]
    async fn whitespace_only_tag_difference_counts_as_unchanged() {
        // merge trims: the Zotero item's " diamond " collapses to "diamond",
        // which the library already has — that must not become a write.
        let sink = FakeSink::new(vec![RemoteItem {
            key: "K1".into(),
            version: 7,
            doi: Some("10.1/a".into()),
            tags: vec![" diamond ".into()],
        }]);
        let report = run(vec![paper("P", "10.1/a", &["diamond"])], &sink, true).await;
        assert_eq!(report.unchanged, 1);
        assert!(sink.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn ambiguous_doi_skips_instead_of_guessing() {
        let sink = FakeSink::new(vec![
            remote("K1", "10.1/a", &["one"]),
            remote("K2", "10.1/a", &["two"]),
        ]);
        let report = run(vec![paper("P", "10.1/a", &["mine"])], &sink, true).await;
        assert_eq!(report.skipped_ambiguous, 1);
        assert!(report.updates.is_empty());
        assert!(sink.writes.lock().unwrap().is_empty());
    }
}
