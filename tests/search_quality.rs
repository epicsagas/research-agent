//! Golden-query guard for search recall.
//!
//! FTS5 matches the words a paper actually contains. Keywords exist to cover
//! the words it does not — the synonym, the expanded acronym, the phrasing a
//! searcher reaches for first. The paraphrase cases below are the whole reason
//! the `keywords` column exists: each one fails before enrichment and passes
//! after, so a regression that stops indexing keywords fails this file rather
//! than silently degrading recall.

use research_agent::adapters::sqlite_store::SqliteStore;
use research_agent::domain::paper::Paper;
use research_agent::ports::index_store::IndexStore;

/// (title, abstract, keywords) — keywords deliberately avoid repeating words
/// already in the title or abstract, matching what enrichment is told to do.
const CORPUS: &[(&str, &str, &str)] = &[
    (
        "Attention Is All You Need",
        "We propose a new simple network architecture based solely on attention mechanisms, dispensing with recurrence and convolutions entirely.",
        "transformer; sequence-to-sequence model; encoder-decoder; NLP architecture",
    ),
    (
        "Linearizability: A Correctness Condition for Concurrent Objects",
        "We introduce linearizability as a correctness condition for concurrent objects that exploits the semantics of abstract data types.",
        "consistency model; strong consistency; atomic operations; distributed systems correctness",
    ),
    (
        "ImageNet Classification with Deep Convolutional Neural Networks",
        "We trained a large, deep convolutional neural network to classify 1.2 million high-resolution images.",
        "AlexNet; computer vision; image recognition; supervised learning",
    ),
    (
        "The Byzantine Generals Problem",
        "Reliable computer systems must handle malfunctioning components that give conflicting information to different parts of the system.",
        "fault tolerance; consensus protocol; BFT; distributed agreement",
    ),
    (
        "A Method for Stochastic Optimization",
        "We introduce Adam, an algorithm for first-order gradient-based optimization of stochastic objective functions.",
        "adaptive learning rate; optimizer; training algorithm; deep learning optimization",
    ),
    (
        "Dynamic Routing Between Capsules",
        "A capsule is a group of neurons whose activity vector represents the instantiation parameters of a specific entity.",
        "capsule network; equivariance; part-whole hierarchy; visual representation",
    ),
    (
        "MapReduce: Simplified Data Processing on Large Clusters",
        "MapReduce is a programming model and an associated implementation for processing and generating large data sets.",
        "batch processing; parallel computation; big data framework; cluster computing",
    ),
    (
        "Bitcoin: A Peer-to-Peer Electronic Cash System",
        "A purely peer-to-peer version of electronic cash would allow online payments to be sent directly from one party to another.",
        "blockchain; cryptocurrency; proof of work; decentralized ledger",
    ),
    (
        "Generative Adversarial Networks",
        "We propose a new framework for estimating generative models via an adversarial process.",
        "GAN; synthetic data generation; discriminator; unsupervised learning",
    ),
    (
        "The Google File System",
        "We have designed and implemented a scalable distributed file system for large distributed data-intensive applications.",
        "GFS; storage system; replication; fault-tolerant storage",
    ),
    (
        "Deep Residual Learning for Image Recognition",
        "We present a residual learning framework to ease the training of networks that are substantially deeper.",
        "ResNet; skip connection; very deep network; vanishing gradient",
    ),
    (
        "Raft: In Search of an Understandable Consensus Algorithm",
        "Raft is a consensus algorithm for managing a replicated log, designed to be more understandable than Paxos.",
        "leader election; log replication; state machine replication; coordination",
    ),
];

/// Seed a store with the corpus. `enriched` controls whether the keyword
/// column is populated — the two states the golden queries compare.
fn seeded_store(enriched: bool) -> (SqliteStore, Vec<String>) {
    let store = SqliteStore::open_in_memory().expect("store");
    let mut ids = Vec::new();
    for (title, abstract_text, keywords) in CORPUS {
        let mut paper = Paper::new((*title).to_string());
        paper.abstract_text = (*abstract_text).to_string();
        store.insert_paper(&paper).expect("insert");
        if enriched {
            store
                .set_paper_keywords(&paper.id, keywords)
                .expect("keywords");
        }
        ids.push(paper.id);
    }
    (store, ids)
}

fn rank_of(store: &SqliteStore, query: &str, paper_id: &str) -> Option<usize> {
    store
        .search_papers(query, 10)
        .expect("search")
        .iter()
        .position(|p| p.id == paper_id)
}

/// Queries whose words appear in the title or abstract. These must work with
/// no keywords at all — if they ever need enrichment, lexical search broke.
#[test]
fn literal_queries_hit_without_any_keywords() {
    let (store, ids) = seeded_store(false);
    for (query, idx) in [
        ("attention mechanisms", 0),
        ("linearizability", 1),
        ("convolutional neural network", 2),
        ("consensus algorithm", 11),
    ] {
        let rank = rank_of(&store, query, &ids[idx]);
        assert!(
            matches!(rank, Some(r) if r < 3),
            "literal query {query:?} should rank {:?} in the top 3, got rank {rank:?}",
            CORPUS[idx].0
        );
    }
}

/// The point of the keywords column: queries using vocabulary the paper never
/// uses. Each must miss before enrichment and hit after.
#[test]
fn paraphrase_queries_need_keywords() {
    let paraphrases = [
        ("transformer", 0),
        ("consistency model", 1),
        ("AlexNet", 2),
        ("fault tolerance", 3),
        ("adaptive learning rate", 4),
        ("blockchain", 7),
    ];

    let (bare, bare_ids) = seeded_store(false);
    for (query, idx) in paraphrases {
        assert!(
            rank_of(&bare, query, &bare_ids[idx]).is_none(),
            "query {query:?} was expected to MISS {:?} without keywords — the \
             fixture no longer proves enrichment adds anything",
            CORPUS[idx].0
        );
    }

    let (rich, rich_ids) = seeded_store(true);
    for (query, idx) in paraphrases {
        let rank = rank_of(&rich, query, &rich_ids[idx]);
        assert!(
            matches!(rank, Some(r) if r < 3),
            "query {query:?} should rank {:?} in the top 3 once enriched, got rank {rank:?}",
            CORPUS[idx].0
        );
    }
}

/// Keywords add recall without displacing literal matches. Both papers must
/// surface near the top: the one carrying the query in its title and the one
/// carrying it only in keywords.
///
/// This deliberately does not assert which of the two ranks first. FTS5 scores
/// with bm25 over the whole row, so a short document beats a long one on term
/// density regardless of which column matched — the shorter AlexNet row
/// currently outranks ResNet even though ResNet has the title match. Asserting
/// a fixed order here would encode a bm25 tuning detail as a contract.
#[test]
fn keyword_matches_do_not_displace_literal_matches() {
    let (store, ids) = seeded_store(true);
    // "image recognition" is in ResNet's title and in AlexNet's keywords.
    let results = store
        .search_papers("image recognition", 10)
        .expect("search");
    let resnet = results.iter().position(|p| p.id == ids[10]);
    let alexnet = results.iter().position(|p| p.id == ids[2]);
    assert!(
        matches!(resnet, Some(r) if r < 3),
        "the title match must stay in the top 3, got {resnet:?}"
    );
    assert!(
        matches!(alexnet, Some(r) if r < 3),
        "the keyword match must stay in the top 3, got {alexnet:?}"
    );
}

/// Re-enriching replaces rather than appends, and the index follows.
#[test]
fn rewriting_keywords_drops_the_old_terms() {
    let (store, ids) = seeded_store(true);
    assert!(rank_of(&store, "transformer", &ids[0]).is_some());

    store
        .set_paper_keywords(&ids[0], "attention model; neural machine translation")
        .expect("rewrite");

    assert!(
        rank_of(&store, "transformer", &ids[0]).is_none(),
        "the replaced keyword should no longer match"
    );
    assert!(
        rank_of(&store, "neural machine translation", &ids[0]).is_some(),
        "the new keyword should match"
    );
}
