/// Additive migrations for pre-existing databases. Each entry is `(sql, column)`
/// where `column` is checked via `PRAGMA table_info` before running, so each
/// migration is idempotent — it runs only when the column is actually missing.
/// This never crashes on "duplicate column" even if `_meta.schema_version` lags
/// behind reality (e.g. a DB upgraded by an earlier build that added the column
/// without bumping the version).
pub const MIGRATION_SQL: &[(&str, &str)] = &[
    ("ALTER TABLE papers ADD COLUMN rating INTEGER", "rating"),
    (
        "ALTER TABLE papers ADD COLUMN openalex_id TEXT",
        "openalex_id",
    ),
    (
        "ALTER TABLE papers ADD COLUMN keywords TEXT NOT NULL DEFAULT ''",
        "keywords",
    ),
];

/// Rebuilds `papers_fts` so it carries the `keywords` column, plus the three
/// triggers that feed it. Unlike `MIGRATION_SQL` this cannot be guarded by a
/// column check: `papers_fts` is a virtual table created with
/// `IF NOT EXISTS`, so an existing database keeps its old 4-column definition
/// forever unless the table is dropped and recreated. Runs only when the
/// stored schema version lags `TARGET_SCHEMA_VERSION` — the trailing `rebuild`
/// re-indexes the whole corpus, which is O(corpus), not something to repeat on
/// every open. Must run AFTER the `keywords` column exists: the recreated
/// triggers reference `new.keywords`.
pub const FTS_V3_SQL: &str = r#"
DROP TRIGGER IF EXISTS papers_ai;
DROP TRIGGER IF EXISTS papers_ad;
DROP TRIGGER IF EXISTS papers_au;
DROP TABLE IF EXISTS papers_fts;

CREATE VIRTUAL TABLE papers_fts USING fts5(
    title, abstract_text, notes, tags, keywords,
    content=papers, content_rowid=rowid, tokenize='trigram'
);

CREATE TRIGGER papers_ai AFTER INSERT ON papers BEGIN
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags, keywords)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags, new.keywords);
END;

CREATE TRIGGER papers_ad AFTER DELETE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags, keywords)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags, old.keywords);
END;

CREATE TRIGGER papers_au AFTER UPDATE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags, keywords)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags, old.keywords);
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags, keywords)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags, new.keywords);
END;

INSERT INTO papers_fts(papers_fts) VALUES('rebuild');
"#;

/// `_meta.schema_version` written once `MIGRATION_SQL` is fully applied.
pub const TARGET_SCHEMA_VERSION: i64 = 3;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version', '1');

CREATE TABLE IF NOT EXISTS papers (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    authors TEXT NOT NULL DEFAULT '[]',
    abstract_text TEXT NOT NULL DEFAULT '',
    year INTEGER,
    venue TEXT,
    doi TEXT,
    arxiv_id TEXT,
    s2_id TEXT,
    openalex_id TEXT,
    url TEXT,
    pdf_path TEXT,
    status TEXT NOT NULL DEFAULT 'discovered',
    reading_status TEXT NOT NULL DEFAULT 'unread',
    notes TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    relevance_score REAL NOT NULL DEFAULT 0.5,
    rating INTEGER,
    keywords TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS papers_fts USING fts5(
    title, abstract_text, notes, tags, keywords,
    content=papers, content_rowid=rowid, tokenize='trigram'
);

CREATE TABLE IF NOT EXISTS research_topics (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    parent_topic_id TEXT REFERENCES research_topics(id),
    depth INTEGER NOT NULL DEFAULT 0,
    priority REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS topic_papers (
    topic_id TEXT NOT NULL REFERENCES research_topics(id),
    paper_id TEXT NOT NULL REFERENCES papers(id),
    relevance REAL NOT NULL DEFAULT 0.5,
    PRIMARY KEY (topic_id, paper_id)
);

CREATE TABLE IF NOT EXISTS knowledge_gaps (
    id TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    topic_id TEXT NOT NULL REFERENCES research_topics(id),
    gap_type TEXT NOT NULL DEFAULT 'missing_literature',
    priority REAL NOT NULL DEFAULT 0.5,
    discovered_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS research_state (
    topic_id TEXT PRIMARY KEY REFERENCES research_topics(id),
    papers_read INTEGER NOT NULL DEFAULT 0,
    papers_queued INTEGER NOT NULL DEFAULT 0,
    gaps_identified INTEGER NOT NULL DEFAULT 0,
    coverage_score REAL NOT NULL DEFAULT 0.0,
    last_updated TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS research_reports (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    topic_ids TEXT NOT NULL DEFAULT '[]',
    content TEXT NOT NULL DEFAULT '',
    format TEXT NOT NULL DEFAULT 'markdown',
    output_path TEXT,
    generated_at TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS papers_ai AFTER INSERT ON papers BEGIN
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags, keywords)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags, new.keywords);
END;

CREATE TRIGGER IF NOT EXISTS papers_ad AFTER DELETE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags, keywords)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags, old.keywords);
END;

CREATE TRIGGER IF NOT EXISTS papers_au AFTER UPDATE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags, keywords)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags, old.keywords);
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags, keywords)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags, new.keywords);
END;

-- Full extracted body text lives outside `papers` so the Paper domain type and
-- every tool response stay small; FTS5 searches it through its own index.
-- Paper-to-paper reference edges (citation graph). Logical keys only: a
-- referenced paper may be ingested in the same call as the citing one, so
-- foreign keys would force an ordering the fetch cannot guarantee.
CREATE TABLE IF NOT EXISTS citations (
    citing_paper_id TEXT NOT NULL,
    cited_paper_id TEXT NOT NULL,
    context TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (citing_paper_id, cited_paper_id)
);

CREATE TABLE IF NOT EXISTS paper_bodies (
    paper_id TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
    body TEXT NOT NULL DEFAULT ''
);

CREATE VIRTUAL TABLE IF NOT EXISTS bodies_fts USING fts5(
    body, content=paper_bodies, content_rowid=rowid, tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS bodies_ai AFTER INSERT ON paper_bodies BEGIN
    INSERT INTO bodies_fts(rowid, body) VALUES (new.rowid, new.body);
END;

CREATE TRIGGER IF NOT EXISTS bodies_ad AFTER DELETE ON paper_bodies BEGIN
    INSERT INTO bodies_fts(bodies_fts, rowid, body)
    VALUES('delete', old.rowid, old.body);
END;

CREATE TRIGGER IF NOT EXISTS bodies_au AFTER UPDATE ON paper_bodies BEGIN
    INSERT INTO bodies_fts(bodies_fts, rowid, body)
    VALUES('delete', old.rowid, old.body);
    INSERT INTO bodies_fts(rowid, body) VALUES (new.rowid, new.body);
END;
"#;
