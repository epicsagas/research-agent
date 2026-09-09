//! Hybrid search: FTS5 lexical hits fused (RRF) with vector hits from a
//! locally-embedded corpus. The vector side is llm-kernel's `TurbovecIndex`
//! persisted next to the database; ids in the index are paper rowids.
//!
//! Hybrid is strictly best-effort: any failure to build the embedding backend
//! or the index degrades to plain lexical search (never an error surfaced to
//! the user), and `sync` (run by `open`) brings the index in line with the
//! corpus incrementally, embedding only new or changed papers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use llm_kernel::embedding::{
    EmbeddingModel, EmbeddingProvider, FastembedProvider, TurbovecIndex, VectorIndex,
};
use llm_kernel::search::{SearchResult, rrf_fuse};

use crate::config::SearchConfig;
use crate::domain::paper::Paper;
use crate::error::Result;
use crate::ports::index_store::IndexStore;

const RRF_K: u32 = 60;
/// Quantization level for the on-disk index (4-bit = recommended recall).
const BIT_WIDTH: u8 = 4;
/// Fetch headroom per channel before fusing and truncating.
const OVERSAMPLE: usize = 2;
/// Cap per-document text fed to the embedder. Models truncate at their
/// context window (~512 tokens); feeding an untruncated PDF body makes the
/// ONNX runtime allocate attention over the raw sequence — observed at
/// 22+ GB RSS for a 53-paper corpus. 2000 chars covers the window; text past
/// it was never represented in the vector anyway.
const MAX_DOC_CHARS: usize = 2000;
/// Runtime clamp for the configured batch size (`[search] embed_batch_size`).
const MIN_BATCH: usize = 8;
const MAX_BATCH: usize = 32;

pub struct HybridSearch {
    backend: Box<dyn EmbeddingProvider>,
    index: TurbovecIndex,
    index_path: PathBuf,
    batch: usize,
}

impl HybridSearch {
    /// Open (and sync if stale) the hybrid search stack. Returns `None` when
    /// hybrid search is unavailable — misconfigured provider, missing API
    /// key, or embedding model not loadable — and the caller should fall
    /// back to lexical search.
    pub fn open(
        store: &dyn IndexStore,
        cfg: Option<&SearchConfig>,
        index_path: PathBuf,
    ) -> Option<Self> {
        let cfg = cfg.cloned().unwrap_or_default();
        let backend = make_backend(&cfg)?;
        let model_id = backend.name().to_string();

        let index = match TurbovecIndex::load(&index_path) {
            Ok(index) => index,
            Err(_) => TurbovecIndex::with_meta(
                backend.dim(),
                BIT_WIDTH,
                Some(model_id.clone()),
                Some("doc_prefix".into()),
                Some(1),
            )
            .ok()?,
        };

        let mut search = Self {
            backend,
            index,
            index_path,
            batch: cfg.embed_batch_size.clamp(MIN_BATCH, MAX_BATCH),
        };
        // A stale index (model changed, papers added/removed/updated since
        // the last build) is brought in line rather than served
        // inconsistently.
        search.sync(store).ok()?;
        Some(search)
    }

    /// Re-embed the whole corpus and persist a fresh index. Texts are
    /// truncated and embedded in batches to bound inference memory
    /// (MAX_DOC_CHARS, `batch`).
    pub fn rebuild(&mut self, store: &dyn IndexStore) -> Result<()> {
        let corpus = store.vector_corpus()?;
        let mut index = self.fresh_index()?;
        let pairs: Vec<(u64, String)> = corpus
            .iter()
            .map(|(rowid, text)| (*rowid as u64, doc_text(text)))
            .collect();
        let entries: Vec<(u64, u64)> = pairs
            .iter()
            .map(|(id, text)| (*id, text_hash(text)))
            .collect();
        add_batched(self.backend.as_ref(), self.batch, &mut index, &pairs)?;
        self.save(index, &entries)?;
        Ok(())
    }

    /// Bring the persisted index in line with the corpus, embedding only
    /// new or changed papers when the on-disk state allows it. Falls back to
    /// a full [`rebuild`](Self::rebuild) on model change, missing state, or
    /// divergence it cannot repair incrementally.
    pub fn sync(&mut self, store: &dyn IndexStore) -> Result<()> {
        let corpus = store.vector_corpus()?;
        if self
            .index
            .meta()
            .and_then(|m| m.model_id.clone())
            .is_some_and(|id| id != self.backend.name())
        {
            return self.rebuild(store);
        }
        let Some(entries) = load_state(&state_path(&self.index_path)) else {
            return self.rebuild(store);
        };
        let state: HashMap<u64, u64> = entries.into_iter().collect();
        if self.index.len() != state.len() {
            // Index and state diverged (e.g. an interrupted save) — the
            // count is the only cross-check the index exposes, so start over.
            return self.rebuild(store);
        }

        let mut to_remove: Vec<u64> = state
            .keys()
            .filter(|id| !corpus.iter().any(|(rowid, _)| *rowid as u64 == **id))
            .copied()
            .collect();
        let mut to_add: Vec<(u64, String)> = Vec::new();
        let mut new_entries: Vec<(u64, u64)> = Vec::with_capacity(corpus.len());
        for (rowid, text) in &corpus {
            let id = *rowid as u64;
            let truncated = doc_text(text);
            let hash = text_hash(&truncated);
            match state.get(&id) {
                Some(&h) if h == hash => {}
                Some(_) => {
                    // Text changed: drop the stale vector before re-adding.
                    to_remove.push(id);
                    to_add.push((id, truncated));
                }
                None => to_add.push((id, truncated)),
            }
            new_entries.push((id, hash));
        }
        if to_remove.is_empty() && to_add.is_empty() {
            return Ok(());
        }

        let embed_err = |e: llm_kernel::error::KernelError| {
            crate::error::ResearchError::Source(format!("embedding failed: {e}"))
        };
        for chunk in to_remove.chunks(self.batch) {
            self.index.remove(chunk).map_err(embed_err)?;
        }
        for chunk in to_add.chunks(self.batch) {
            let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
            let vectors: Vec<Vec<f32>> = self
                .backend
                .embed_documents(&texts)
                .map_err(embed_err)?
                .into_iter()
                .map(|r| r.vector)
                .collect();
            let ids: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
            self.index.add_with_ids(&vectors, &ids).map_err(embed_err)?;
        }
        // State before index: an interrupt between the two writes leaves
        // state ahead of the index, which the len check turns into a clean
        // full rebuild on the next sync.
        persist_state(&state_path(&self.index_path), &new_entries)?;
        self.index.save(&self.index_path).map_err(|e| {
            crate::error::ResearchError::Source(format!("vector index save failed: {e}"))
        })?;
        Ok(())
    }

    /// Fresh index carrying the current meta (prefix policy, schema version)
    /// but no vectors from a previous model/build.
    fn fresh_index(&self) -> Result<TurbovecIndex> {
        let (prefix_policy, schema_version) = match self.index.meta() {
            Some(m) => (m.prefix_policy.clone(), m.schema_version),
            None => (None, None),
        };
        TurbovecIndex::with_meta(
            self.backend.dim(),
            BIT_WIDTH,
            Some(self.backend.name().to_string()),
            prefix_policy,
            schema_version,
        )
        .map_err(|e| crate::error::ResearchError::Source(format!("embedding failed: {e}")))
    }

    /// Persist the embedded-state sidecar and the index itself.
    fn save(&mut self, index: TurbovecIndex, entries: &[(u64, u64)]) -> Result<()> {
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        persist_state(&state_path(&self.index_path), entries)?;
        index.save(&self.index_path).map_err(|e| {
            crate::error::ResearchError::Source(format!("vector index save failed: {e}"))
        })?;
        self.index = index;
        Ok(())
    }

    /// Lexical + vector search, RRF-fused. The lexical channel comes from the
    /// store's FTS5 query so body hits and phrase-quoting rules are shared.
    pub fn search(&self, store: &dyn IndexStore, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let fetch = limit * OVERSAMPLE;
        let lexical = store.search_papers(query, fetch)?;

        let qv = self.backend.embed(query).map_err(|e| {
            crate::error::ResearchError::Source(format!("query embedding failed: {e}"))
        })?;
        let hits = self
            .index
            .search(&qv.vector, fetch)
            .map_err(|e| crate::error::ResearchError::Source(format!("vector search: {e}")))?;
        let mut vector = Vec::new();
        for hit in hits {
            if let Some(paper) = store.paper_by_rowid(hit.id as i64)? {
                vector.push(paper);
            }
        }

        let lexical_ids: Vec<SearchResult> = lexical
            .iter()
            .map(|p| SearchResult {
                id: p.id.clone(),
                score: 1.0,
                text: String::new(),
            })
            .collect();
        let vector_ids: Vec<SearchResult> = vector
            .iter()
            .map(|p| SearchResult {
                id: p.id.clone(),
                score: 1.0,
                text: String::new(),
            })
            .collect();

        let fused = rrf_fuse(&[lexical_ids, vector_ids], RRF_K);

        let mut by_id: HashMap<String, Paper> = lexical
            .into_iter()
            .chain(vector)
            .map(|p| (p.id.clone(), p))
            .collect();
        Ok(fused
            .into_iter()
            .filter_map(|hit| by_id.remove(&hit.id))
            .take(limit)
            .collect())
    }
}

/// Truncated document text that actually gets embedded (and hashed): models
/// cut at their context window, so text past MAX_DOC_CHARS never reaches the
/// vector.
fn doc_text(text: &str) -> String {
    text.chars().take(MAX_DOC_CHARS).collect()
}

fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Embedded-state sidecar next to the index: `(rowid, hash of embedded
/// text)` pairs, letting `sync` embed only new or changed papers.
fn state_path(index_path: &Path) -> PathBuf {
    index_path.with_extension("state.json")
}

fn load_state(path: &Path) -> Option<Vec<(u64, u64)>> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn persist_state(path: &Path, entries: &[(u64, u64)]) -> Result<()> {
    let json = serde_json::to_string(entries)
        .map_err(|e| crate::error::ResearchError::Source(format!("state serialize failed: {e}")))?;
    std::fs::write(path, json)
        .map_err(|e| crate::error::ResearchError::Source(format!("state save failed: {e}")))
}

/// Embed `(id, text)` pairs in batches and add them to the index.
fn add_batched(
    backend: &dyn EmbeddingProvider,
    batch: usize,
    index: &mut TurbovecIndex,
    pairs: &[(u64, String)],
) -> Result<()> {
    let embed_err = |e: llm_kernel::error::KernelError| {
        crate::error::ResearchError::Source(format!("embedding failed: {e}"))
    };
    for chunk in pairs.chunks(batch) {
        let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
        let vectors: Vec<Vec<f32>> = backend
            .embed_documents(&texts)
            .map_err(embed_err)?
            .into_iter()
            .map(|r| r.vector)
            .collect();
        let ids: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
        index.add_with_ids(&vectors, &ids).map_err(embed_err)?;
    }
    Ok(())
}

/// Build the embedding backend from `[search]` config: "openai" uses the
/// configured key env (BYOK; unavailable without a key), anything else is the
/// local ONNX model — GPU execution provider where the platform has one,
/// CPU otherwise.
fn make_backend(cfg: &SearchConfig) -> Option<Box<dyn EmbeddingProvider>> {
    if cfg.provider == "openai" {
        let key = std::env::var(&cfg.openai_api_key_env).ok()?;
        return Some(Box::new(
            llm_kernel::embedding::OpenAIEmbeddingClient::new_small(key),
        ));
    }

    let model: EmbeddingModel = cfg.model.parse().ok()?;
    // Model weights live under ~/.research/models (fastembed's CWD default
    // would scatter a .fastembed_cache into whatever directory you run from).
    let cache_dir = Some(crate::config::research_dir().join("models"));

    // Escape hatch: force CPU ONNX when a GPU/ANE execution provider
    // misbehaves (e.g. CoreML memory blowups). Set to anything but "0"/"".
    if std::env::var("RESEARCH_EMBED_CPU")
        .map(|v| !(v.is_empty() || v == "0"))
        .unwrap_or(false)
    {
        return FastembedProvider::new(model, cache_dir)
            .map(|provider| Box::new(provider) as Box<dyn EmbeddingProvider>)
            .ok();
    }

    #[cfg(target_os = "macos")]
    if let Ok(provider) = FastembedProvider::new_with_coreml(model, cache_dir.clone()) {
        return Some(Box::new(provider));
    }
    #[cfg(target_os = "windows")]
    if let Ok(provider) = FastembedProvider::new_with_directml(model, cache_dir.clone()) {
        return Some(Box::new(provider));
    }

    FastembedProvider::new(model, cache_dir)
        .map(|provider| Box::new(provider) as Box<dyn EmbeddingProvider>)
        .ok()
}

/// Resolve the vector index path next to the database file.
pub fn index_path_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("embeddings.idx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_store::SqliteStore;
    use tempfile::tempdir;

    /// Matches the `[search] embed_batch_size` serde default.
    const DEFAULT_BATCH: usize = 16;

    /// Deterministic fake embeddings: dims are [has_apple, has_fruit, 0×6]
    /// (Turbovec quantization needs dim % 8 == 0). The query "apple" is
    /// similar to both the apple doc (lexical+vector hit) and the fruit doc
    /// (vector-only hit). Counts how many documents it has embedded so tests
    /// can assert incremental sync behavior.
    struct FakeProvider(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl FakeProvider {
        fn new() -> Self {
            Self(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)))
        }
    }

    impl EmbeddingProvider for FakeProvider {
        fn dim(&self) -> usize {
            8
        }
        fn embed(
            &self,
            text: &str,
        ) -> llm_kernel::error::Result<llm_kernel::embedding::EmbeddingResult> {
            let lower = text.to_lowercase();
            let v = vec![
                if lower.contains("apple") || lower == "query: apple" {
                    1.0
                } else {
                    0.0
                },
                if lower.contains("apple") || lower.contains("fruit") {
                    1.0
                } else {
                    0.0
                },
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ];
            Ok(llm_kernel::embedding::EmbeddingResult {
                vector: v,
                text_preview: String::new(),
            })
        }
        fn embed_documents(
            &self,
            texts: &[&str],
        ) -> llm_kernel::error::Result<Vec<llm_kernel::embedding::EmbeddingResult>> {
            use std::sync::atomic::Ordering;
            self.0.fetch_add(texts.len(), Ordering::SeqCst);
            texts
                .iter()
                .map(|t| self.embed(t))
                .collect::<llm_kernel::error::Result<Vec<_>>>()
        }
        fn name(&self) -> &str {
            "fake"
        }
    }

    fn hybrid_with_fake(store: &SqliteStore, dir: &tempfile::TempDir) -> HybridSearch {
        let mut search = HybridSearch {
            backend: Box::new(FakeProvider::new()),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: dir.path().join("embeddings.idx"),
            batch: DEFAULT_BATCH,
        };
        search.rebuild(store).unwrap();
        search
    }

    #[test]
    fn fuses_lexical_and_vector_hits() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut apple = Paper::new("Apple growing guide".into());
        apple.abstract_text = "How to grow apples".into();
        let mut fruit = Paper::new("Fruit cultivation".into());
        fruit.abstract_text = "Orchard management".into();
        store.insert_paper(&apple).unwrap();
        store.insert_paper(&fruit).unwrap();

        let dir = tempdir().unwrap();
        let search = hybrid_with_fake(&store, &dir);

        let results = search.search(&store, "apple", 10).unwrap();
        let ids: Vec<&str> = results.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(results.len(), 2, "both lexical and vector hits returned");
        assert_eq!(ids[0], apple.id, "the lexical hit outranks vector-only");
        assert!(ids.contains(&fruit.id.as_str()), "vector-only hit included");
    }

    #[test]
    fn stale_index_rebuilds_via_open() {
        let store = SqliteStore::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("embeddings.idx");

        let search = HybridSearch::open(&store, None, path.clone()).unwrap();
        assert_eq!(search.index.len(), 0);

        let mut paper = Paper::new("Later paper".into());
        paper.abstract_text = "content".into();
        store.insert_paper(&paper).unwrap();

        // Index on disk still says 0 papers while the store has 1 — open()
        // must rebuild rather than serve a stale index.
        let reopened = HybridSearch::open(&store, None, path).unwrap();
        assert_eq!(reopened.index.len(), 1);
    }

    #[test]
    fn rebuild_after_new_ingests_replaces_not_appends() {
        // Regression: rebuild used to append the whole corpus to the existing
        // index, so re-embedding after new ingests hit "id already present"
        // and failed — which open() swallowed into a silent lexical fallback.
        let store = SqliteStore::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        let mut search = HybridSearch {
            backend: Box::new(FakeProvider::new()),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: dir.path().join("embeddings.idx"),
            batch: DEFAULT_BATCH,
        };

        let mut first = Paper::new("First paper".into());
        first.abstract_text = "content".into();
        store.insert_paper(&first).unwrap();
        search.rebuild(&store).unwrap();
        assert_eq!(search.index.len(), 1);

        let mut second = Paper::new("Second paper".into());
        second.abstract_text = "content".into();
        store.insert_paper(&second).unwrap();
        search.rebuild(&store).unwrap();
        assert_eq!(search.index.len(), 2, "fresh rebuild, not an append");
    }

    #[test]
    fn rebuild_batches_documents() {
        // More papers than one batch: every chunk must land in the index.
        let store = SqliteStore::open_in_memory().unwrap();
        for i in 0..(DEFAULT_BATCH * 2 + 1) {
            let mut paper = Paper::new(format!("Paper {i}"));
            paper.abstract_text = "content".into();
            store.insert_paper(&paper).unwrap();
        }
        let dir = tempdir().unwrap();
        let mut search = HybridSearch {
            backend: Box::new(FakeProvider::new()),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: dir.path().join("embeddings.idx"),
            batch: DEFAULT_BATCH,
        };
        search.rebuild(&store).unwrap();
        assert_eq!(search.index.len(), DEFAULT_BATCH * 2 + 1);
    }

    #[test]
    fn sync_embeds_only_new_or_changed_papers() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut a = Paper::new("Apple growing guide".into());
        a.abstract_text = "How to grow apples".into();
        let mut b = Paper::new("Fruit cultivation".into());
        b.abstract_text = "Orchard management".into();
        store.insert_paper(&a).unwrap();
        store.insert_paper(&b).unwrap();

        let dir = tempdir().unwrap();
        let provider = FakeProvider::new();
        let embedded = provider.0.clone();
        let mut search = HybridSearch {
            backend: Box::new(provider),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: dir.path().join("embeddings.idx"),
            batch: DEFAULT_BATCH,
        };
        search.rebuild(&store).unwrap();
        let after_rebuild = embedded.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(after_rebuild, 2);

        // One new paper: sync must embed exactly 1 doc, not the corpus.
        let mut c = Paper::new("Later paper".into());
        c.abstract_text = "content".into();
        store.insert_paper(&c).unwrap();
        search.sync(&store).unwrap();
        assert_eq!(search.index.len(), 3);
        assert_eq!(
            embedded.load(std::sync::atomic::Ordering::SeqCst) - after_rebuild,
            1,
            "only the new paper is embedded"
        );

        // Changed body: same paper count, one doc re-embedded.
        store.set_paper_body(&b.id, "brand new body text").unwrap();
        search.sync(&store).unwrap();
        assert_eq!(search.index.len(), 3);
        assert_eq!(
            embedded.load(std::sync::atomic::Ordering::SeqCst) - after_rebuild,
            2,
            "only the changed paper is re-embedded"
        );
    }

    #[test]
    fn sync_falls_back_to_rebuild_on_state_divergence() {
        // A state file whose entry count disagrees with the index (interrupted
        // save) cannot be repaired incrementally — sync must full-rebuild.
        let store = SqliteStore::open_in_memory().unwrap();
        let mut a = Paper::new("Paper A".into());
        a.abstract_text = "content".into();
        let mut b = Paper::new("Paper B".into());
        b.abstract_text = "content".into();
        store.insert_paper(&a).unwrap();
        store.insert_paper(&b).unwrap();

        let dir = tempdir().unwrap();
        let provider = FakeProvider::new();
        let embedded = provider.0.clone();
        let index_path = dir.path().join("embeddings.idx");
        let mut search = HybridSearch {
            backend: Box::new(provider),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: index_path.clone(),
            batch: DEFAULT_BATCH,
        };
        search.rebuild(&store).unwrap();
        let after_rebuild = embedded.load(std::sync::atomic::Ordering::SeqCst);

        std::fs::write(
            state_path(&index_path),
            "[[1,111],[2,222],[3,333]]", // 3 entries, index has 2
        )
        .unwrap();
        search.sync(&store).unwrap();

        assert_eq!(
            search.index.len(),
            2,
            "corpus size wins over diverged state"
        );
        assert_eq!(
            embedded.load(std::sync::atomic::Ordering::SeqCst) - after_rebuild,
            2,
            "divergence triggers a full re-embed"
        );
    }

    #[test]
    fn unknown_model_degrades_to_none() {
        let store = SqliteStore::open_in_memory().unwrap();
        let dir = tempdir().unwrap();
        let cfg = SearchConfig {
            model: "NotAModel".into(),
            ..SearchConfig::default()
        };
        assert!(HybridSearch::open(&store, Some(&cfg), dir.path().join("e.idx")).is_none());
    }
}
