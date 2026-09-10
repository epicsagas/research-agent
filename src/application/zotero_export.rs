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
use crate::domain::paper::Paper;
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
    // Papers are grouped by DOI before any write: siblings sharing a DOI
    // target one Zotero item, so their tag sets must merge into a single
    // write. Writing per paper would let the second paper's tags be judged
    // against a tag set the first one already pushed, and silently dropped.
    let mut groups: Vec<(String, Vec<&Paper>)> = Vec::new();
    let mut group_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let papers = store.list_papers(None)?;
    for paper in &papers {
        // Stored DOIs are normalized at import; normalizing again is free and
        // keeps matching exact even for rows that predate that guarantee.
        let Some(doi) = paper.doi.as_deref().and_then(normalize_doi) else {
            report.skipped_no_doi += 1;
            continue;
        };
        if !by_doi.contains_key(&doi) {
            report.skipped_not_in_zotero += 1;
            continue;
        }
        match group_of.get(&doi) {
            Some(&idx) => groups[idx].1.push(paper),
            None => {
                group_of.insert(doi.clone(), groups.len());
                groups.push((doi, vec![paper]));
            }
        }
    }

    for (doi, members) in groups {
        let items = &by_doi[&doi];
        let &[item] = items.as_slice() else {
            report.skipped_ambiguous += members.len();
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
        // Union of every sibling's tags, so one write carries them all.
        let mut merged = current.clone();
        for paper in &members {
            merged = merge_tags(&merged, &paper.tags);
        }
        if merged == current {
            report.unchanged += members.len();
            continue;
        }
        // New-tag count measured against the trimmed tag set merge started
        // from: item.tags may carry whitespace-only entries merge drops, so
        // subtracting its length could underflow.
        let added = merged.len() - current.len();
        let titles: Vec<String> = members
            .iter()
            .map(|p| format!("{}: +{added} tag(s)", p.title))
            .collect();
        if !apply {
            report.updates.extend(titles);
            continue;
        }
        if sink.write_tags(&item.key, item.version, &merged).await? {
            report.updates.extend(titles);
        } else {
            // Version conflict: the item changed in Zotero since the read.
            // Retrying with the same stale version is doomed, so every
            // sibling on this DOI is reported skipped, not re-attempted.
            report.skipped_conflict += members.len();
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[tokio::test]
    async fn update_count_ignores_whitespace_only_item_tags() {
        // The item's two whitespace-only tags are dropped by merge; counting
        // against raw item.tags would underflow usize. New-tag count is 1.
        let sink = FakeSink::new(vec![RemoteItem {
            key: "K1".into(),
            version: 7,
            doi: Some("10.1/a".into()),
            tags: vec![" ".into(), "  ".into()],
        }]);
        let report = run(vec![paper("P", "10.1/a", &["mine"])], &sink, true).await;
        assert_eq!(report.updates, vec!["P: +1 tag(s)"]);
    }

    #[tokio::test]
    async fn sibling_papers_on_one_doi_contribute_all_their_tags() {
        // P1 and P2 share a DOI, so they target the same Zotero item. One
        // write must carry both tag sets: judging P2 against the set P1 just
        // pushed would drop "y" silently.
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &["old"])]);
        let report = run(
            vec![paper("P1", "10.1/a", &["x"]), paper("P2", "10.1/a", &["y"])],
            &sink,
            true,
        )
        .await;
        let writes = sink.writes.lock().unwrap();
        assert_eq!(writes.len(), 1, "one item, one write");
        // Sibling order follows the store's row order, which is not part of
        // the contract; only the tag set is.
        let mut written = writes[0].1.clone();
        written.sort();
        assert_eq!(written, vec!["old", "x", "y"]);
        assert_eq!(report.updates.len(), 2, "both papers reported");
    }

    #[tokio::test]
    async fn sibling_papers_adding_nothing_are_all_unchanged() {
        let sink = FakeSink::new(vec![remote("K1", "10.1/a", &["same"])]);
        let report = run(
            vec![
                paper("P1", "10.1/a", &["same"]),
                paper("P2", "10.1/a", &["same"]),
            ],
            &sink,
            true,
        )
        .await;
        assert_eq!(report.unchanged, 2);
        assert!(sink.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn ambiguous_doi_counts_every_sibling_paper() {
        let sink = FakeSink::new(vec![
            remote("K1", "10.1/a", &["one"]),
            remote("K2", "10.1/a", &["two"]),
        ]);
        let report = run(
            vec![paper("P1", "10.1/a", &["x"]), paper("P2", "10.1/a", &["y"])],
            &sink,
            false,
        )
        .await;
        assert_eq!(report.skipped_ambiguous, 2);
    }

    #[tokio::test]
    async fn conflicting_item_is_not_retried_for_sibling_doi() {
        // Both papers map to K1; the first write conflicts, so the second
        // must skip without another doomed HTTP attempt.
        let mut sink = FakeSink::new(vec![remote("K1", "10.1/a", &["old"])]);
        sink.conflicts.push("K1".into());
        let report = run(
            vec![
                paper("P1", "10.1/a", &["mine"]),
                paper("P2", "10.1/a", &["mine"]),
            ],
            &sink,
            true,
        )
        .await;
        assert_eq!(report.skipped_conflict, 2);
        assert_eq!(sink.writes.lock().unwrap().len(), 1);
    }
}
