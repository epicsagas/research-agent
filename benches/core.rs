//! Search-stack benchmarks (SPEC-20260910111449, R1).
//!
//! Informational only: numbers vary by machine, so nothing here gates CI on
//! an absolute threshold. The corpus is deterministic and the embedding
//! provider is a hash-based fake — no model download, no ONNX, no network —
//! so the numbers measure the search stack, not the hardware.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use criterion::{criterion_group, criterion_main, Criterion};
use research_agent::adapters::sqlite_store::SqliteStore;
use research_agent::application::hybrid_search::HybridSearch;
use research_agent::domain::paper::Paper;
use research_agent::ports::index_store::IndexStore;

const CORPUS: usize = 500;
const QUERY: &str = "quantum sensing";

const TOPICS: &[&str] = &[
    "quantum sensing",
    "ion trap",
    "photonic crystal",
    "graph neural network",
    "protein folding",
    "climate model",
    "topological insulator",
    "federated learning",
    "cognitive bias",
    "market microstructure",
];

const METHODS: &[&str] = &[
    "a large-scale survey",
    "a controlled experiment",
    "a new architecture",
    "a theoretical analysis",
    "an empirical evaluation",
];

/// Deterministic hash-spread embeddings, 8 dims (Turbovec quantization needs
/// dim % 8 == 0). Each token lights one dim, so distinct topics land in
/// distinct regions and hybrid queries return real fused hits.
struct HashEmbed;

impl EmbeddingProvider for HashEmbed {
    fn dim(&self) -> usize {
        8
    }

    fn embed(&self, text: &str) -> llm_kernel::error::Result<llm_kernel::embedding::EmbeddingResult> {
        let mut v = vec![0.0f32; 8];
        for token in text.to_lowercase().split_whitespace() {
            let mut h = DefaultHasher::new();
            token.hash(&mut h);
            v[(h.finish() % 8) as usize] += 1.0;
        }
        Ok(llm_kernel::embedding::EmbeddingResult {
            vector: v,
            text_preview: String::new(),
        })
    }

    fn embed_documents(
        &self,
        texts: &[&str],
    ) -> llm_kernel::error::Result<Vec<llm_kernel::embedding::EmbeddingResult>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn name(&self) -> &str {
        "bench-hash"
    }
}

use llm_kernel::embedding::EmbeddingProvider;

struct Fixture {
    _dir: tempfile::TempDir,
    store: SqliteStore,
    hybrid: HybridSearch,
}

/// One shared fixture for all groups: a temp DB seeded with a deterministic
/// 500-paper corpus (50 of them carrying section-marked bodies).
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SqliteStore::open(&dir.path().join("bench.db")).expect("store");
        for i in 0..CORPUS {
            let topic = TOPICS[i % TOPICS.len()];
            let method = METHODS[i % METHODS.len()];
            let mut paper = Paper::new(format!("{topic}: study {i:03}"));
            paper.abstract_text =
                format!("We study {topic} through {method}. Results cover {topic} benchmarks.");
            paper.year = Some(2000 + (i % 26) as u32);
            store.insert_paper(&paper).expect("insert");
            if i % 10 == 0 {
                let body = format!(
                    "## Introduction\nBackground on {topic}.\n\n## Methods\n{method} with a nanodiamond probe.\n\n## Readout\nThe {topic} readout fidelity improved across trials.\n\n## Conclusion\n{topic} results are promising.\n"
                );
                store.set_paper_body(&paper.id, &body).expect("body");
            }
        }
        let hybrid = HybridSearch::with_parts(
            Box::new(HashEmbed),
            dir.path().join("bench-embeddings.idx"),
        );
        Fixture {
            _dir: dir,
            store,
            hybrid,
        }
    })
}

fn bench_lexical_query(c: &mut Criterion) {
    let fx = fixture();
    c.bench_function("lexical_query/fts5_500", |b| {
        b.iter(|| fx.store.search_papers(QUERY, 20).expect("search"))
    });
}

fn bench_hybrid_query(c: &mut Criterion) {
    let fx = fixture();
    c.bench_function("hybrid_query/rrf_500", |b| {
        b.iter(|| fx.hybrid.search(&fx.store, QUERY, 20).expect("search"))
    });
}

fn bench_index_rebuild(c: &mut Criterion) {
    let fx = fixture();
    let path = fx._dir.path().join("bench-rebuild.idx");
    let mut hybrid = HybridSearch::with_parts(Box::new(HashEmbed), path);
    c.bench_function("index_rebuild/500_docs", |b| {
        b.iter(|| hybrid.rebuild(&fx.store).expect("rebuild"))
    });
}

fn bench_body_evidence(c: &mut Criterion) {
    let fx = fixture();
    c.bench_function("body_evidence/snippet_50", |b| {
        b.iter(|| {
            fx.store
                .search_body_evidence("nanodiamond readout", None, 10)
                .expect("evidence")
        })
    });
}

criterion_group!(
    benches,
    bench_lexical_query,
    bench_hybrid_query,
    bench_index_rebuild,
    bench_body_evidence
);
criterion_main!(benches);
