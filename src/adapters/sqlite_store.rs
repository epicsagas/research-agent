use std::sync::Mutex;

use rusqlite::{Connection, params};

use crate::domain::knowledge_gap::{GapType, KnowledgeGap};
use crate::domain::paper::{Paper, PaperStatus, Rating, ReadingStatus};
use crate::domain::research_report::ResearchReport;
use crate::domain::research_state::ResearchState;
use crate::domain::research_topic::ResearchTopic;
use crate::error::{ResearchError, Result};
use crate::ports::index_store::IndexStore;
use crate::store::schema::{MIGRATION_SQL, SCHEMA_SQL, TARGET_SCHEMA_VERSION};

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

/// Quote every whitespace-separated token as an FTS5 phrase so user input
/// like `latch-free` or `worst-case` is matched literally instead of being
/// parsed as FTS5 syntax (where `-` is the NOT operator and bare `case`
/// becomes a column reference). Double quotes in the input are dropped;
/// tokens are implicitly AND-ed.
fn fts_phrase_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

impl SqliteStore {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        // Wait up to 5s on a locked DB instead of failing immediately. This
        // matters under the MCP server, where concurrent tool calls each open
        // their own store handle and run `init_schema` (CREATE TABLE …) —
        // without a busy timeout the parallel writers race on the SQLite write
        // lock and surface "database is locked". Harmless for the single-handle
        // CLI path.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn paper_from_row(row: &rusqlite::Row<'_>) -> std::result::Result<Paper, rusqlite::Error> {
        let authors_str: String = row.get("authors")?;
        let tags_str: String = row.get("tags")?;
        Ok(Paper {
            id: row.get("id")?,
            title: row.get("title")?,
            authors: serde_json::from_str(&authors_str).unwrap_or_default(),
            abstract_text: row.get("abstract_text")?,
            year: row.get("year")?,
            venue: row.get("venue")?,
            doi: row.get("doi")?,
            arxiv_id: row.get("arxiv_id")?,
            s2_id: row.get("s2_id")?,
            url: row.get("url")?,
            pdf_path: row.get("pdf_path")?,
            status: {
                let s: String = row.get("status")?;
                PaperStatus::from_str_lossy(&s)
            },
            notes: row.get("notes")?,
            tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            relevance_score: row.get("relevance_score")?,
            reading_status: {
                let s: String = row.get("reading_status")?;
                ReadingStatus::from_str_lossy(&s)
            },
            rating: {
                // Defensive read: a corrupt or out-of-range value (hand-edited
                // DB, an older binary) maps to None (unrated) instead of
                // failing the query. Rating::new enforces 1..=5 on every write
                // path, so this only affects data that bypassed the constructor.
                let raw: Option<i64> = row.get("rating")?;
                raw.and_then(|n| u8::try_from(n).ok().and_then(|v| Rating::new(v).ok()))
            },
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

impl IndexStore for SqliteStore {
    fn insert_paper(&self, paper: &Paper) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO papers
             (id, title, authors, abstract_text, year, venue, doi, arxiv_id, s2_id,
              url, pdf_path, status, reading_status, notes, tags, relevance_score,
              rating, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                paper.id,
                paper.title,
                serde_json::to_string(&paper.authors)?,
                paper.abstract_text,
                paper.year,
                paper.venue,
                paper.doi,
                paper.arxiv_id,
                paper.s2_id,
                paper.url,
                paper.pdf_path,
                paper.status.as_str(),
                paper.reading_status.as_str(),
                paper.notes,
                serde_json::to_string(&paper.tags)?,
                paper.relevance_score,
                paper.rating.map(|r| r.get() as i64),
                paper.created_at,
                paper.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_paper(&self, id: &str) -> Result<Option<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT * FROM papers WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::paper_from_row(row)?)),
            None => Ok(None),
        }
    }

    fn update_paper_status(&self, id: &str, status: PaperStatus) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let changed = conn.execute(
            "UPDATE papers SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now, id],
        )?;
        if changed == 0 {
            return Err(ResearchError::NotFound(format!("paper {id}")));
        }
        Ok(())
    }

    fn update_reading_status(&self, id: &str, status: ReadingStatus) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let changed = conn.execute(
            "UPDATE papers SET reading_status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now, id],
        )?;
        if changed == 0 {
            return Err(ResearchError::NotFound(format!("paper {id}")));
        }
        Ok(())
    }

    fn update_rating(&self, id: &str, rating: Rating) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let changed = conn.execute(
            "UPDATE papers SET rating = ?1, updated_at = ?2 WHERE id = ?3",
            params![rating.get() as i64, now, id],
        )?;
        if changed == 0 {
            return Err(ResearchError::NotFound(format!("paper {id}")));
        }
        Ok(())
    }

    fn clear_rating(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let changed = conn.execute(
            "UPDATE papers SET rating = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(ResearchError::NotFound(format!("paper {id}")));
        }
        Ok(())
    }

    fn search_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p
             JOIN papers_fts fts ON fts.rowid = p.rowid
             WHERE papers_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let limit_i64 = limit as i64;
        let fts_query = fts_phrase_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let rows = stmt.query_map(params![fts_query, limit_i64], Self::paper_from_row)?;
        let mut papers = Vec::new();
        for p in rows {
            papers.push(p?);
        }
        Ok(papers)
    }

    fn list_papers(&self, limit: Option<usize>) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let sql = match limit {
            Some(n) => format!("SELECT * FROM papers ORDER BY created_at DESC LIMIT {n}"),
            None => "SELECT * FROM papers ORDER BY created_at DESC".into(),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::paper_from_row)?;
        let mut papers = Vec::new();
        for p in rows {
            papers.push(p?);
        }
        Ok(papers)
    }

    fn list_papers_by_topic(&self, topic_id: &str, limit: Option<usize>) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        // Interpolating `limit` as a usize is safe (no injection surface) and
        // matches the existing `list_papers` style; the user-supplied value is
        // the parameterized `topic_id`.
        let sql = match limit {
            Some(n) => format!(
                "SELECT p.* FROM papers p
                 JOIN topic_papers tp ON tp.paper_id = p.id
                 WHERE tp.topic_id = ?1
                 ORDER BY tp.relevance DESC, p.created_at DESC
                 LIMIT {n}"
            ),
            None => "SELECT p.* FROM papers p
                 JOIN topic_papers tp ON tp.paper_id = p.id
                 WHERE tp.topic_id = ?1
                 ORDER BY tp.relevance DESC, p.created_at DESC"
                .into(),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![topic_id], Self::paper_from_row)?;
        let mut papers = Vec::new();
        for p in rows {
            papers.push(p?);
        }
        Ok(papers)
    }

    fn insert_topic(&self, topic: &ResearchTopic) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO research_topics
             (id, name, description, parent_topic_id, depth, priority, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                topic.id,
                topic.name,
                topic.description,
                topic.parent_topic_id,
                topic.depth,
                topic.priority,
                topic.created_at,
            ],
        )?;
        Ok(())
    }

    fn get_topic(&self, id: &str) -> Result<Option<ResearchTopic>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT * FROM research_topics WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(ResearchTopic {
                id: row.get("id")?,
                name: row.get("name")?,
                description: row.get("description")?,
                parent_topic_id: row.get("parent_topic_id")?,
                depth: row.get("depth")?,
                priority: row.get("priority")?,
                created_at: row.get("created_at")?,
            })),
            None => Ok(None),
        }
    }

    fn list_topics(&self) -> Result<Vec<ResearchTopic>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt =
            conn.prepare("SELECT * FROM research_topics ORDER BY depth ASC, name ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(ResearchTopic {
                id: row.get("id")?,
                name: row.get("name")?,
                description: row.get("description")?,
                parent_topic_id: row.get("parent_topic_id")?,
                depth: row.get("depth")?,
                priority: row.get("priority")?,
                created_at: row.get("created_at")?,
            })
        })?;
        let mut topics = Vec::new();
        for t in rows {
            topics.push(t?);
        }
        Ok(topics)
    }

    fn link_paper_to_topic(&self, paper_id: &str, topic_id: &str, relevance: f32) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO topic_papers (topic_id, paper_id, relevance) VALUES (?1, ?2, ?3)",
            params![topic_id, paper_id, relevance],
        )?;
        Ok(())
    }

    fn insert_gap(&self, gap: &KnowledgeGap) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO knowledge_gaps
             (id, description, topic_id, gap_type, priority, discovered_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                gap.id,
                gap.description,
                gap.topic_id,
                gap.gap_type.as_str(),
                gap.priority,
                gap.discovered_at,
            ],
        )?;
        Ok(())
    }

    fn list_gaps(&self, topic_id: Option<&str>) -> Result<Vec<KnowledgeGap>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut gaps = Vec::new();
        match topic_id {
            Some(tid) => {
                let mut stmt = conn.prepare(
                    "SELECT * FROM knowledge_gaps WHERE topic_id = ?1 ORDER BY priority DESC",
                )?;
                let rows = stmt.query_map(params![tid], |row| {
                    let gt: String = row.get("gap_type")?;
                    Ok(KnowledgeGap {
                        id: row.get("id")?,
                        description: row.get("description")?,
                        topic_id: row.get("topic_id")?,
                        gap_type: GapType::from_str_lossy(&gt),
                        priority: row.get("priority")?,
                        discovered_at: row.get("discovered_at")?,
                    })
                })?;
                for g in rows {
                    gaps.push(g?);
                }
            }
            None => {
                let mut stmt =
                    conn.prepare("SELECT * FROM knowledge_gaps ORDER BY priority DESC")?;
                let rows = stmt.query_map([], |row| {
                    let gt: String = row.get("gap_type")?;
                    Ok(KnowledgeGap {
                        id: row.get("id")?,
                        description: row.get("description")?,
                        topic_id: row.get("topic_id")?,
                        gap_type: GapType::from_str_lossy(&gt),
                        priority: row.get("priority")?,
                        discovered_at: row.get("discovered_at")?,
                    })
                })?;
                for g in rows {
                    gaps.push(g?);
                }
            }
        }
        Ok(gaps)
    }

    fn get_research_state(&self, topic_id: &str) -> Result<Option<ResearchState>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT * FROM research_state WHERE topic_id = ?1")?;
        let mut rows = stmt.query(params![topic_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(ResearchState {
                topic_id: row.get("topic_id")?,
                papers_read: row.get("papers_read")?,
                papers_queued: row.get("papers_queued")?,
                gaps_identified: row.get("gaps_identified")?,
                coverage_score: row.get("coverage_score")?,
                last_updated: row.get("last_updated")?,
            })),
            None => Ok(None),
        }
    }

    fn update_research_state(&self, state: &ResearchState) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO research_state
             (topic_id, papers_read, papers_queued, gaps_identified, coverage_score, last_updated)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                state.topic_id,
                state.papers_read,
                state.papers_queued,
                state.gaps_identified,
                state.coverage_score,
                state.last_updated,
            ],
        )?;
        Ok(())
    }

    fn insert_report(&self, report: &ResearchReport) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO research_reports
             (id, title, topic_ids, content, format, output_path, generated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                report.id,
                report.title,
                serde_json::to_string(&report.topic_ids)?,
                report.to_markdown(),
                report.format,
                report.output_path,
                report.generated_at,
            ],
        )?;
        Ok(())
    }

    fn list_reports(&self, limit: Option<usize>) -> Result<Vec<ResearchReport>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let sql = match limit {
            Some(n) => {
                format!("SELECT * FROM research_reports ORDER BY generated_at DESC LIMIT {n}")
            }
            None => "SELECT * FROM research_reports ORDER BY generated_at DESC".into(),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let ids_str: String = row.get("topic_ids")?;
            let content: String = row.get("content").unwrap_or_default();
            let sections = ResearchReport::parse_sections(&content);
            Ok(ResearchReport {
                id: row.get("id")?,
                title: row.get("title")?,
                topic_ids: serde_json::from_str(&ids_str).unwrap_or_default(),
                sections,
                format: row.get("format")?,
                output_path: row.get("output_path")?,
                generated_at: row.get("generated_at")?,
            })
        })?;
        let mut reports = Vec::new();
        for r in rows {
            reports.push(r?);
        }
        Ok(reports)
    }

    fn rebuild_index(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute("INSERT INTO papers_fts(papers_fts) VALUES('rebuild')", [])?;
        Ok(())
    }

    fn init_schema(&self) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute_batch(SCHEMA_SQL)?;
        // Apply additive migrations only when the stored version lags. Each
        // migration is guarded by a column-existence check, so it is idempotent
        // even if a prior build already added the column without bumping the
        // version (which would otherwise make `ALTER` fail on "duplicate
        // column"). The work and the version bump share one transaction.
        let current_version = Self::schema_version(&conn);
        if current_version < TARGET_SCHEMA_VERSION {
            let tx = conn.transaction()?;
            for (sql, column) in MIGRATION_SQL {
                if !Self::column_exists(&tx, "papers", column)? {
                    tx.execute_batch(sql)?;
                }
            }
            tx.execute(
                "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
                params![TARGET_SCHEMA_VERSION.to_string()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }
}

impl SqliteStore {
    /// Read the stored schema version, defaulting to 0 if the `_meta` row is
    /// missing or non-numeric (which triggers migration — the safe direction).
    fn schema_version(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| Ok(row.get::<_, String>(0)?.parse::<i64>().unwrap_or(0)),
        )
        .unwrap_or(0)
    }

    /// True if `column` exists on `table` (via PRAGMA table_info). Errors
    /// propagate (do NOT swallow as "column missing") — a PRAGMA failure under
    /// lock contention or I/O error must surface, not be misread as "run the
    /// ALTER" and crash on "duplicate column".
    fn column_exists(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        // PRAGMA table_info columns: cid, name, type, notnull, dflt_value, pk.
        while let Some(row) = rows.next()? {
            if row
                .get::<_, String>(1)
                .map(|name| name == column)
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::knowledge_gap::GapType;

    fn test_store() -> SqliteStore {
        SqliteStore::open_in_memory().unwrap()
    }

    fn schema_version(store: &SqliteStore) -> i64 {
        let conn = store.conn.lock().unwrap();
        SqliteStore::schema_version(&conn)
    }

    #[test]
    fn init_schema_idempotent() {
        let store = test_store();
        store.init_schema().unwrap();
        store.init_schema().unwrap();
    }

    #[test]
    fn insert_and_get_paper() {
        let store = test_store();
        let paper = Paper::new("Attention Is All You Need".into());
        store.insert_paper(&paper).unwrap();

        let got = store.get_paper(&paper.id).unwrap().unwrap();
        assert_eq!(got.title, "Attention Is All You Need");
        assert_eq!(got.status, PaperStatus::Discovered);
    }

    #[test]
    fn get_missing_paper_returns_none() {
        let store = test_store();
        assert!(store.get_paper("nonexistent").unwrap().is_none());
    }

    #[test]
    fn update_paper_status() {
        let store = test_store();
        let paper = Paper::new("Test".into());
        store.insert_paper(&paper).unwrap();

        store
            .update_paper_status(&paper.id, PaperStatus::Read)
            .unwrap();
        let got = store.get_paper(&paper.id).unwrap().unwrap();
        assert_eq!(got.status, PaperStatus::Read);
    }

    #[test]
    fn update_reading_status() {
        let store = test_store();
        let paper = Paper::new("Test".into());
        store.insert_paper(&paper).unwrap();

        store
            .update_reading_status(&paper.id, ReadingStatus::Completed)
            .unwrap();
        let got = store.get_paper(&paper.id).unwrap().unwrap();
        assert_eq!(got.reading_status, ReadingStatus::Completed);
    }

    #[test]
    fn update_status_missing_paper_errors() {
        let store = test_store();
        let result = store.update_paper_status("missing", PaperStatus::Read);
        assert!(result.is_err());
    }

    #[test]
    fn search_papers_fts() {
        let store = test_store();
        let mut paper = Paper::new("Deep Learning for NLP".into());
        paper.abstract_text = "A survey of deep learning methods".into();
        store.insert_paper(&paper).unwrap();

        let results = store.search_papers("deep learning", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Deep Learning for NLP");
    }

    #[test]
    fn search_papers_fts_hyphen_and_keyword_tokens() {
        let store = test_store();
        let mut paper = Paper::new("GTX: A Write-Optimized Latch-free Graph Data System".into());
        paper.abstract_text = "worst-case optimal join".into();
        store.insert_paper(&paper).unwrap();

        // `-` is the FTS5 NOT operator and `case` is otherwise parsed as a column.
        for q in [
            "GTX latch-free",
            "worst-case optimal",
            "latch-free \"graph\"",
        ] {
            let results = store.search_papers(q, 10).unwrap();
            assert_eq!(results.len(), 1, "query {q:?} must match literally");
        }
        assert!(store.search_papers("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn list_papers_with_limit() {
        let store = test_store();
        for i in 0..5 {
            let p = Paper::new(format!("Paper {i}"));
            store.insert_paper(&p).unwrap();
        }
        let all = store.list_papers(None).unwrap();
        assert_eq!(all.len(), 5);
        let limited = store.list_papers(Some(3)).unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[test]
    fn topic_crud() {
        let store = test_store();
        let topic = ResearchTopic::new("Transformers".into());
        store.insert_topic(&topic).unwrap();

        let got = store.get_topic(&topic.id).unwrap().unwrap();
        assert_eq!(got.name, "Transformers");

        let topics = store.list_topics().unwrap();
        assert_eq!(topics.len(), 1);
    }

    #[test]
    fn link_paper_to_topic() {
        let store = test_store();
        let paper = Paper::new("Test Paper".into());
        let topic = ResearchTopic::new("Topic".into());
        store.insert_paper(&paper).unwrap();
        store.insert_topic(&topic).unwrap();

        store
            .link_paper_to_topic(&paper.id, &topic.id, 0.9)
            .unwrap();
    }

    #[test]
    fn list_papers_by_topic_returns_only_linked_papers() {
        let store = test_store();
        let topic_a = ResearchTopic::new("Topic A".into());
        let topic_b = ResearchTopic::new("Topic B".into());
        store.insert_topic(&topic_a).unwrap();
        store.insert_topic(&topic_b).unwrap();

        let paper_a = Paper::new("Paper A".into());
        let paper_b = Paper::new("Paper B".into());
        let paper_unlinked = Paper::new("Paper Unlinked".into());
        store.insert_paper(&paper_a).unwrap();
        store.insert_paper(&paper_b).unwrap();
        store.insert_paper(&paper_unlinked).unwrap();

        store
            .link_paper_to_topic(&paper_a.id, &topic_a.id, 0.9)
            .unwrap();
        store
            .link_paper_to_topic(&paper_b.id, &topic_b.id, 0.9)
            .unwrap();

        let a_papers = store.list_papers_by_topic(&topic_a.id, None).unwrap();
        assert_eq!(a_papers.len(), 1);
        assert_eq!(a_papers[0].id, paper_a.id);

        let b_papers = store.list_papers_by_topic(&topic_b.id, None).unwrap();
        assert_eq!(b_papers.len(), 1);
        assert_eq!(b_papers[0].id, paper_b.id);
    }

    #[test]
    fn list_papers_by_topic_empty_for_topic_with_no_papers() {
        // Regression: a topic with no linked papers must return an empty list,
        // not arbitrary papers. The pre-fix gaps/report code path used
        // `list_papers(Some(N))` which would have leaked unrelated papers here.
        let store = test_store();
        let topic_with = ResearchTopic::new("With Papers".into());
        let topic_without = ResearchTopic::new("Empty Topic".into());
        store.insert_topic(&topic_with).unwrap();
        store.insert_topic(&topic_without).unwrap();

        let paper = Paper::new("Some Paper".into());
        store.insert_paper(&paper).unwrap();
        store
            .link_paper_to_topic(&paper.id, &topic_with.id, 0.5)
            .unwrap();

        let empty = store.list_papers_by_topic(&topic_without.id, None).unwrap();
        assert!(
            empty.is_empty(),
            "topic with no linked papers must return empty, not arbitrary papers"
        );
    }

    #[test]
    fn list_papers_by_topic_orders_by_relevance_then_respects_limit() {
        let store = test_store();
        let topic = ResearchTopic::new("Topic".into());
        store.insert_topic(&topic).unwrap();

        let hi = Paper::new("High Relevance".into());
        let lo = Paper::new("Low Relevance".into());
        store.insert_paper(&hi).unwrap();
        store.insert_paper(&lo).unwrap();
        store.link_paper_to_topic(&hi.id, &topic.id, 0.9).unwrap();
        store.link_paper_to_topic(&lo.id, &topic.id, 0.1).unwrap();

        let ordered = store.list_papers_by_topic(&topic.id, None).unwrap();
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].id, hi.id, "most relevant first");

        let limited = store.list_papers_by_topic(&topic.id, Some(1)).unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].id, hi.id);
    }

    #[test]
    fn gap_crud() {
        let store = test_store();
        let topic = ResearchTopic::new("Topic".into());
        store.insert_topic(&topic).unwrap();

        let gap = KnowledgeGap::new(
            "Missing survey".into(),
            topic.id.clone(),
            GapType::MissingLiterature,
        );
        store.insert_gap(&gap).unwrap();

        let gaps = store.list_gaps(Some(&topic.id)).unwrap();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].description, "Missing survey");

        let all_gaps = store.list_gaps(None).unwrap();
        assert_eq!(all_gaps.len(), 1);
    }

    #[test]
    fn research_state_upsert() {
        let store = test_store();
        let topic = ResearchTopic::new("Topic".into());
        store.insert_topic(&topic).unwrap();

        let state = ResearchState {
            topic_id: topic.id.clone(),
            papers_read: 5,
            papers_queued: 3,
            gaps_identified: 2,
            coverage_score: 0.6,
            last_updated: chrono::Utc::now().to_rfc3339(),
        };
        store.update_research_state(&state).unwrap();

        let got = store.get_research_state(&topic.id).unwrap().unwrap();
        assert_eq!(got.papers_read, 5);
        assert_eq!(got.coverage_score, 0.6);
    }

    #[test]
    fn report_crud() {
        let store = test_store();
        let mut report = ResearchReport::new("Report".into(), vec!["t1".into()]);
        report
            .sections
            .push(crate::domain::research_report::ReportSection {
                heading: "Intro".into(),
                content: "Hello world".into(),
            });
        store.insert_report(&report).unwrap();

        let reports = store.list_reports(None).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].title, "Report");
        assert_eq!(reports[0].sections.len(), 1);
        assert_eq!(reports[0].sections[0].heading, "Intro");
        assert_eq!(reports[0].sections[0].content, "Hello world");
    }

    #[test]
    fn rebuild_index() {
        let store = test_store();
        let mut p = Paper::new("Rebuild Test".into());
        p.abstract_text = "Testing rebuild".into();
        store.insert_paper(&p).unwrap();

        store.rebuild_index().unwrap();
        let results = store.search_papers("rebuild", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn update_rating_roundtrip() {
        let store = test_store();
        let paper = Paper::new("Rated Paper".into());
        store.insert_paper(&paper).unwrap();
        store
            .update_rating(&paper.id, Rating::new(4).unwrap())
            .unwrap();
        let got = store.get_paper(&paper.id).unwrap().unwrap();
        assert_eq!(got.rating.map(Rating::get), Some(4));
    }

    #[test]
    fn update_rating_missing_paper_errors() {
        let store = test_store();
        assert!(
            store
                .update_rating("missing", Rating::new(3).unwrap())
                .is_err()
        );
    }

    #[test]
    fn topic_hierarchy_depth() {
        let store = test_store();
        let parent = ResearchTopic::new("ML".into());
        store.insert_topic(&parent).unwrap();

        let child = ResearchTopic::new_subtopic("Deep Learning".into(), &parent);
        store.insert_topic(&child).unwrap();

        let got = store.get_topic(&child.id).unwrap().unwrap();
        assert_eq!(got.parent_topic_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(got.depth, 1);
    }

    #[test]
    fn list_topics_orders_parents_before_children() {
        let store = test_store();
        // Parent sorts after the child by name, so a pure name sort would list
        // the child first — depth-first ordering must put the parent above.
        let parent = ResearchTopic::new("Zoo".into());
        store.insert_topic(&parent).unwrap();
        let child = ResearchTopic::new_subtopic("Ant".into(), &parent);
        store.insert_topic(&child).unwrap();

        let topics = store.list_topics().unwrap();
        let parent_pos = topics.iter().position(|t| t.id == parent.id).unwrap();
        let child_pos = topics.iter().position(|t| t.id == child.id).unwrap();
        assert!(parent_pos < child_pos);
    }

    #[test]
    fn init_schema_marks_target_version_and_is_idempotent() {
        let store = test_store();
        assert_eq!(schema_version(&store), TARGET_SCHEMA_VERSION);
        // Re-running is a no-op (no "duplicate column" error path).
        store.init_schema().unwrap();
        store.init_schema().unwrap();
        assert_eq!(schema_version(&store), TARGET_SCHEMA_VERSION);
    }

    #[test]
    fn init_schema_migrates_legacy_v0_database() {
        // Simulate a pre-rating database: drop the rating column and reset the
        // stored version to 0, then confirm init_schema re-applies the migration.
        let store = test_store();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute_batch("ALTER TABLE papers DROP COLUMN rating")
                .unwrap();
            conn.execute_batch("UPDATE _meta SET value = '0' WHERE key = 'schema_version'")
                .unwrap();
        }
        assert_eq!(schema_version(&store), 0);

        store.init_schema().unwrap();

        assert_eq!(schema_version(&store), TARGET_SCHEMA_VERSION);
        let paper = Paper::new("Legacy".into());
        store.insert_paper(&paper).unwrap();
        store
            .update_rating(&paper.id, Rating::new(5).unwrap())
            .unwrap();
        let got = store.get_paper(&paper.id).unwrap().unwrap();
        assert_eq!(got.rating.map(Rating::get), Some(5));
    }

    #[test]
    fn init_schema_tolerates_rating_present_but_version_zero() {
        // Regression: PR #5 added the rating column via ALTER but never bumped
        // schema_version, leaving real DBs in the state {rating present,
        // version='0'}. A naive re-run of the ALTER crashes on "duplicate
        // column". init_schema must tolerate this, bump the version, and keep
        // the existing column — not crash every command.
        let store = test_store();
        {
            let conn = store.conn.lock().unwrap();
            // rating already exists from the fresh schema; just reset the version.
            conn.execute_batch("UPDATE _meta SET value = '0' WHERE key = 'schema_version'")
                .unwrap();
        }
        assert_eq!(schema_version(&store), 0);

        store.init_schema().unwrap();

        assert_eq!(schema_version(&store), TARGET_SCHEMA_VERSION);
        let paper = Paper::new("Regession".into());
        store.insert_paper(&paper).unwrap();
        store
            .update_rating(&paper.id, Rating::new(4).unwrap())
            .unwrap();
    }
}
