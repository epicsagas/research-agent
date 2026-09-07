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
];

/// `_meta.schema_version` written once `MIGRATION_SQL` is fully applied.
pub const TARGET_SCHEMA_VERSION: i64 = 2;

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
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS papers_fts USING fts5(
    title, abstract_text, notes, tags,
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
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS papers_ad AFTER DELETE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags);
END;

CREATE TRIGGER IF NOT EXISTS papers_au AFTER UPDATE ON papers BEGIN
    INSERT INTO papers_fts(papers_fts, rowid, title, abstract_text, notes, tags)
    VALUES('delete', old.rowid, old.title, old.abstract_text, old.notes, old.tags);
    INSERT INTO papers_fts(rowid, title, abstract_text, notes, tags)
    VALUES (new.rowid, new.title, new.abstract_text, new.notes, new.tags);
END;

-- Full extracted body text lives outside `papers` so the Paper domain type and
-- every tool response stay small; FTS5 searches it through its own index.
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
