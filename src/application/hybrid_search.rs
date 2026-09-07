//! Hybrid search: FTS5 lexical hits fused (RRF) with vector hits from a
//! locally-embedded corpus. The vector side is llm-kernel's `TurbovecIndex`
//! persisted next to the database; ids in the index are paper rowids.
//!
//! Hybrid is strictly best-effort: any failure to build the embedding backend
//! or the index degrades to plain lexical search (never an error surfaced to
//! the user), and `index --rebuild` / `index_rebuild` force a re-embed.

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

pub struct HybridSearch {
    backend: Box<dyn EmbeddingProvider>,
    index: TurbovecIndex,
    index_path: PathBuf,
}

impl HybridSearch {
    /// Open (and repair/rebuild if stale) the hybrid search stack. Returns
    /// `None` when hybrid search is unavailable — misconfigured provider,
    /// missing API key, or embedding model not loadable — and the caller
    /// should fall back to lexical search.
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

        // A stale index (different model, or papers added since the last
        // build) is rebuilt in place rather than served inconsistently.
        let corpus_len = store.vector_corpus().ok()?.len();
        let stale = index
            .meta()
            .and_then(|m| m.model_id.clone())
            .is_some_and(|id| id != model_id)
            || index.len() != corpus_len;
        if stale {
            let mut search = Self {
                backend,
                index,
                index_path,
            };
            search.rebuild(store).ok()?;
            return Some(search);
        }

        Some(Self {
            backend,
            index,
            index_path,
        })
    }

    /// Re-embed the whole corpus and persist the index.
    pub fn rebuild(&mut self, store: &dyn IndexStore) -> Result<()> {
        let corpus = store.vector_corpus()?;
        let texts: Vec<&str> = corpus.iter().map(|(_, text)| text.as_str()).collect();
        let embed_err = |e: llm_kernel::error::KernelError| {
            crate::error::ResearchError::Source(format!("embedding failed: {e}"))
        };
        let vectors: Vec<Vec<f32>> = self
            .backend
            .embed_documents(&texts)
            .map_err(embed_err)?
            .into_iter()
            .map(|r| r.vector)
            .collect();
        let ids: Vec<u64> = corpus.iter().map(|(rowid, _)| *rowid as u64).collect();
        self.index.add_with_ids(&vectors, &ids).map_err(embed_err)?;
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        self.index.save(&self.index_path).map_err(|e| {
            crate::error::ResearchError::Source(format!("vector index save failed: {e}"))
        })?;
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

    /// Deterministic fake embeddings: dims are [has_apple, has_fruit, 0×6]
    /// (Turbovec quantization needs dim % 8 == 0). The query "apple" is
    /// similar to both the apple doc (lexical+vector hit) and the fruit doc
    /// (vector-only hit).
    struct FakeProvider;

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
        fn name(&self) -> &str {
            "fake"
        }
    }

    fn hybrid_with_fake(store: &SqliteStore, dir: &tempfile::TempDir) -> HybridSearch {
        let mut search = HybridSearch {
            backend: Box::new(FakeProvider),
            index: TurbovecIndex::with_meta(8, BIT_WIDTH, Some("fake".into()), None, Some(1))
                .unwrap(),
            index_path: dir.path().join("embeddings.idx"),
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
