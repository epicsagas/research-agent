use std::sync::Mutex;

use rusqlite::{Connection, params};

use crate::domain::citation::Citation;
use crate::domain::knowledge_gap::{GapType, KnowledgeGap};
use crate::domain::paper::{Paper, PaperStatus, Rating, ReadingStatus};
use crate::domain::research_report::ResearchReport;
use crate::domain::research_state::ResearchState;
use crate::domain::research_topic::ResearchTopic;
use crate::error::{ResearchError, Result};
use crate::ports::index_store::{BodyEvidence, IndexStore};
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

/// Where in `body` an FTS5 `snippet()` result came from.
///
/// Anchoring on the matched term alone is wrong twice over: the term may occur
/// many times (`find` would take the first, not the one FTS chose), and the
/// trigram tokenizer matches inside words, so a hit on "ion" can land in the
/// middle of the heading "Introduction" and report a truncated section.
///
/// Rebuilding the snippet's own text and locating *that* pins the real
/// position: it is a contiguous run of the body, long enough to be unique in
/// practice. Ellipses mark where snippet() clipped the window, so the
/// unclipped middle is what gets matched.
fn locate_snippet(snippet: &str, body: &str) -> Option<usize> {
    // Strip exactly the pair of ellipses snippet() adds to mark a clipped
    // window. `trim_matches` would also eat any the body text itself starts or
    // ends with; that happens to come out even today because the needle and the
    // lead shrink together, but the compensation is incidental and stating the
    // intent directly costs nothing.
    let core = snippet.strip_prefix('…').unwrap_or(snippet);
    let core = core.strip_suffix('…').unwrap_or(core);
    // Trim before measuring, not after. `lead` and the text located in the body
    // must be counted against the *same* string: measuring the lead against an
    // untrimmed window while searching for its trimmed text shifts the anchor
    // right by every character the trim removed, and PDF bodies routinely keep
    // indentation on wrapped lines.
    let core = core.trim();
    let plain: String = core.chars().filter(|c| *c != '[' && *c != ']').collect();
    let plain = plain.as_str();
    if plain.is_empty() {
        return None;
    }
    // Offset the snippet start by where the first match sits inside it, so the
    // anchor is the matched text itself rather than the snippet's leading edge
    // (which can begin mid-heading and truncate the section name).
    let lead = core
        .find('[')
        .map(|b| core[..b].chars().filter(|c| *c != '[' && *c != ']').count());
    let in_snippet = lead
        .map(|chars| plain.chars().take(chars).map(char::len_utf8).sum())
        .unwrap_or(0);
    if let Some(pos) = body.find(plain) {
        return Some(pos + in_snippet);
    }
    // snippet() reproduces the body's casing, so an exact hit is the norm.
    // Fall back case-insensitively rather than silently anchoring to offset 0,
    // which would report the document's first section for a match anywhere.
    let lower_body = body.to_lowercase();
    let pos = lower_body.find(&plain.to_lowercase())? + in_snippet;
    // Byte offsets from the lowercased copy are only valid if lowercasing did
    // not change the length; give up rather than report a wrong anchor.
    (lower_body.len() == body.len()).then_some(pos)
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
            openalex_id: row.get("openalex_id")?,
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
              openalex_id, url, pdf_path, status, reading_status, notes, tags,
              relevance_score, rating, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
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
                paper.openalex_id,
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

    fn find_paper_by_doi(&self, doi: &str) -> Result<Option<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        // Compare on the normalized form so a DOI stored raw by an earlier
        // ingest (Europe PMC and Semantic Scholar store what upstream sends)
        // still matches a normalized one arriving now. Normalizing only the
        // incoming side would let 10.1038/NATURE12373 and 10.1038/nature12373
        // coexist as separate papers.
        let needle = crate::adapters::bib_importer::normalize_doi(doi);
        let Some(needle) = needle else {
            return Ok(None);
        };
        // Normalize the stored side with the same function, not a parallel SQL
        // `replace()` chain: the chain silently covered fewer prefixes than
        // `normalize_doi`, so `doi:` and `http://doi.org/` rows never matched.
        // `doi:` cannot be expressed as a `replace()` anyway without corrupting
        // a DOI that contains the substring. Prefiltering on a suffix match
        // keeps SQLite from handing back the whole table.
        let mut stmt = conn.prepare(
            "SELECT * FROM papers
             WHERE doi IS NOT NULL AND lower(trim(doi)) LIKE '%' || ?1",
        )?;
        let mut rows = stmt.query(params![needle])?;
        while let Some(row) = rows.next()? {
            let stored: Option<String> = row.get("doi")?;
            let matches = stored
                .as_deref()
                .and_then(crate::adapters::bib_importer::normalize_doi)
                .is_some_and(|stored| stored == needle);
            if matches {
                return Ok(Some(Self::paper_from_row(row)?));
            }
        }
        Ok(None)
    }

    fn find_paper_by_openalex_id(&self, openalex_id: &str) -> Result<Option<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT * FROM papers WHERE openalex_id = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![openalex_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::paper_from_row(row)?)),
            None => Ok(None),
        }
    }

    fn set_paper_body(&self, paper_id: &str, body: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO paper_bodies (paper_id, body) VALUES (?1, ?2)",
            params![paper_id, body],
        )?;
        Ok(())
    }

    fn get_paper_body(&self, paper_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT body FROM paper_bodies WHERE paper_id = ?1")?;
        let mut rows = stmt.query(params![paper_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    fn vector_corpus(&self) -> Result<Vec<(i64, String)>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare(
            "SELECT p.rowid, p.title, p.abstract_text, pb.body
             FROM papers p
             LEFT JOIN paper_bodies pb ON pb.paper_id = p.id",
        )?;
        let rows = stmt.query_map([], |row| {
            let title: String = row.get(1)?;
            let abstract_text: String = row.get(2)?;
            let body: Option<String> = row.get(3)?;
            let text = match body {
                Some(b) if !b.is_empty() => format!("{title}\n{abstract_text}\n{b}"),
                _ => format!("{title}\n{abstract_text}"),
            };
            Ok((row.get::<_, i64>(0)?, text))
        })?;
        let mut corpus = Vec::new();
        for row in rows {
            corpus.push(row?);
        }
        Ok(corpus)
    }

    fn paper_by_rowid(&self, rowid: i64) -> Result<Option<Paper>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare("SELECT * FROM papers WHERE rowid = ?1")?;
        let mut rows = stmt.query(params![rowid])?;
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
        let fts_query = fts_phrase_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let limit_i64 = limit as i64;

        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p
             JOIN papers_fts fts ON fts.rowid = p.rowid
             WHERE papers_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit_i64], Self::paper_from_row)?;
        let mut papers = Vec::new();
        for p in rows {
            papers.push(p?);
        }

        // Body hits: papers whose stored body text matches but whose title /
        // abstract / notes did not. Ranks across two FTS tables are not
        // comparable, so body-only hits simply follow the metadata hits.
        // ponytail: append-after ordering; a cross-table rank fusion only pays
        // off once libraries grow past a few thousand papers.
        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p
             JOIN paper_bodies pb ON pb.paper_id = p.id
             JOIN bodies_fts fts ON fts.rowid = pb.rowid
             WHERE bodies_fts MATCH ?1
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit_i64], Self::paper_from_row)?;
        for p in rows {
            let p = p?;
            if !papers.iter().any(|existing| existing.id == p.id) {
                papers.push(p);
            }
        }
        papers.truncate(limit);
        Ok(papers)
    }

    fn search_body_evidence(
        &self,
        query: &str,
        paper_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BodyEvidence>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let fts_query = fts_phrase_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        // snippet() gives the matching window with terms bracketed; the body
        // itself comes back so the match can be located for anchoring.
        // Scoping happens in SQL, not after the fact: filtering a whole-library
        // result set in the caller lets other papers' hits crowd out the
        // requested paper's before it is ever reached.
        let mut stmt = conn.prepare(
            "SELECT p.id, p.title, pb.body,
                    snippet(bodies_fts, 0, '[', ']', '…', 32) AS snip
             FROM papers p
             JOIN paper_bodies pb ON pb.paper_id = p.id
             JOIN bodies_fts fts ON fts.rowid = pb.rowid
             WHERE bodies_fts MATCH ?1
               AND (?2 IS NULL OR p.id = ?2)
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![fts_query, paper_id, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (paper_id, title, body, snippet) = row?;
            // Anchor on the matched term itself, which snippet() brackets.
            // Anchoring on surrounding context instead would land on the
            // leading edge of the window and can sit *before* the very heading
            // the match falls under.
            let anchor = locate_snippet(&snippet, &body)
                .map(|off| crate::domain::anchor::resolve(&body, off))
                .unwrap_or_default();
            out.push(BodyEvidence {
                paper_id,
                title,
                snippet,
                anchor,
            });
        }
        Ok(out)
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

    fn insert_citations(&self, citations: &[Citation]) -> Result<usize> {
        let mut conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let tx = conn.transaction()?;
        let mut inserted = 0usize;
        for c in citations {
            // INSERT OR IGNORE: the pair is the PK, so re-running a reference
            // fetch is a no-op for edges already stored.
            let n = tx.execute(
                "INSERT OR IGNORE INTO citations (citing_paper_id, cited_paper_id, context)
                 VALUES (?1, ?2, ?3)",
                params![c.citing_paper_id, c.cited_paper_id, c.context],
            )?;
            inserted += n;
        }
        tx.commit()?;
        Ok(inserted)
    }

    fn citations_for_paper(&self, paper_id: &str) -> Result<Vec<Citation>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare(
            "SELECT citing_paper_id, cited_paper_id, context
             FROM citations WHERE citing_paper_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![paper_id], |row| {
            Ok(Citation {
                citing_paper_id: row.get(0)?,
                cited_paper_id: row.get(1)?,
                context: row.get(2)?,
            })
        })?;
        let mut citations = Vec::new();
        for c in rows {
            citations.push(c?);
        }
        Ok(citations)
    }

    fn citations_citing_paper(&self, paper_id: &str) -> Result<Vec<Citation>> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let mut stmt = conn.prepare(
            "SELECT citing_paper_id, cited_paper_id, context
             FROM citations WHERE cited_paper_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![paper_id], |row| {
            Ok(Citation {
                citing_paper_id: row.get(0)?,
                cited_paper_id: row.get(1)?,
                context: row.get(2)?,
            })
        })?;
        let mut citations = Vec::new();
        for c in rows {
            citations.push(c?);
        }
        Ok(citations)
    }

    fn set_citation_contexts(&self, citations: &[Citation]) -> Result<usize> {
        let mut conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let tx = conn.transaction()?;
        let mut updated = 0usize;
        for c in citations {
            // Scoped to existing edges: labeling never invents an edge the
            // graph sync did not establish.
            updated += tx.execute(
                "UPDATE citations SET context = ?3
                 WHERE citing_paper_id = ?1 AND cited_paper_id = ?2",
                params![c.citing_paper_id, c.cited_paper_id, c.context],
            )?;
        }
        tx.commit()?;
        Ok(updated)
    }

    fn rebuild_index(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            ResearchError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        conn.execute("INSERT INTO papers_fts(papers_fts) VALUES('rebuild')", [])?;
        conn.execute("INSERT INTO bodies_fts(bodies_fts) VALUES('rebuild')", [])?;
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
    fn citations_roundtrip_and_dedupe() {
        let store = test_store();
        let citing = Paper::new("citing".into());
        let cited = Paper::new("cited".into());
        store.insert_paper(&citing).unwrap();
        store.insert_paper(&cited).unwrap();

        let edge = Citation::new(citing.id.clone(), cited.id.clone());
        // Inserting the same edge twice must be a no-op the second time.
        let first = [edge.clone()];
        assert_eq!(store.insert_citations(&first).unwrap(), 1);
        assert_eq!(store.insert_citations(&first).unwrap(), 0);

        let edges = store.citations_for_paper(&citing.id).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].cited_paper_id, cited.id);
        assert!(store.citations_for_paper("unknown").unwrap().is_empty());
    }

    #[test]
    fn find_paper_by_openalex_id() {
        let store = test_store();
        let mut paper = Paper::new("oa paper".into());
        paper.openalex_id = Some("W2741809807".into());
        store.insert_paper(&paper).unwrap();

        let got = store.find_paper_by_openalex_id("W2741809807").unwrap();
        assert_eq!(got.unwrap().id, paper.id);
        assert!(store.find_paper_by_openalex_id("W1").unwrap().is_none());
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
    fn body_text_is_stored_searched_and_readable() {
        let store = test_store();
        let paper = Paper::new("Invisible Title".into());
        store.insert_paper(&paper).unwrap();
        assert!(store.search_papers("quantum", 10).unwrap().is_empty());

        store
            .set_paper_body(
                &paper.id,
                "## Introduction\nThe quantum Lich equation dominates.",
            )
            .unwrap();

        // Body-only term finds the paper even though title/abstract don't match.
        let hits = store.search_papers("quantum", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, paper.id);
        assert_eq!(hits[0].title, "Invisible Title");

        // Roundtrip: replacing the body works, reading it back works.
        store
            .set_paper_body(&paper.id, "## Results\nCompletely different body.")
            .unwrap();
        assert!(store.search_papers("quantum", 10).unwrap().is_empty());
        let body = store.get_paper_body(&paper.id).unwrap().unwrap();
        assert!(body.contains("## Results"));
        assert!(store.get_paper_body("missing-id").unwrap().is_none());
    }

    #[test]
    fn find_paper_by_doi() {
        let store = test_store();
        let mut paper = Paper::new("Doi Paper".into());
        paper.doi = Some("10.1/findme".into());
        store.insert_paper(&paper).unwrap();

        assert_eq!(
            store.find_paper_by_doi("10.1/findme").unwrap().unwrap().id,
            paper.id
        );
        assert!(store.find_paper_by_doi("10.1/missing").unwrap().is_none());
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

    /// A body hit must say where in the paper it matched, not just which
    /// paper — that is the whole point of storing bodies.
    #[test]
    fn body_evidence_carries_snippet_and_anchor() {
        let store = test_store();
        let paper = Paper::new("thermometry paper".into());
        store.insert_paper(&paper).unwrap();
        let body = "<!-- page 1 -->\nintro\n## Methods\nwe used nanodiamond probes\n<!-- page 2 -->\n## Results\nthe readout was stable\n";
        store.set_paper_body(&paper.id, body).unwrap();

        let hits = store.search_body_evidence("nanodiamond", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].paper_id, paper.id);
        assert!(hits[0].snippet.contains("nanodiamond"));
        assert_eq!(hits[0].anchor.section.as_deref(), Some("Methods"));
        assert_eq!(hits[0].anchor.page, Some(1));

        let hits = store.search_body_evidence("readout", None, 10).unwrap();
        assert_eq!(hits[0].anchor.section.as_deref(), Some("Results"));
        assert_eq!(hits[0].anchor.page, Some(2));
    }

    #[test]
    fn body_evidence_empty_for_nonmatching_or_blank_query() {
        let store = test_store();
        let paper = Paper::new("p".into());
        store.insert_paper(&paper).unwrap();
        store
            .set_paper_body(&paper.id, "## Intro\nsome text")
            .unwrap();

        assert!(
            store
                .search_body_evidence("absent", None, 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .search_body_evidence("   ", None, 10)
                .unwrap()
                .is_empty()
        );
    }

    /// The returned offset is the matched text, not the snippet's leading
    /// edge: a snippet that opens mid-heading would otherwise anchor inside
    /// the heading and report a truncated section name.
    #[test]
    fn locate_snippet_points_at_the_match_not_the_window() {
        let body = "## Intro\nalpha text\n## Results\nbeta text here\n";
        let at = locate_snippet("…## Results\nbeta [text] here…", body).unwrap();
        assert_eq!(&body[at..at + 4], "text");
        // That offset sits after the heading, so the section resolves whole.
        assert_eq!(
            crate::domain::anchor::resolve(body, at).section.as_deref(),
            Some("Results")
        );

        assert_eq!(locate_snippet("…", body), None);
        assert_eq!(locate_snippet("text absent from body", body), None);
    }

    /// A snippet window that opens with indented body text must still anchor on
    /// the matched term. PDF bodies keep indentation on wrapped lines, so a
    /// leading-whitespace window is routine rather than exotic.
    #[test]
    fn locate_snippet_anchors_through_leading_whitespace() {
        let body = "<!-- page 1 -->\n## Methods\n    we used nanodiamond probes here\n";
        let at = locate_snippet("…    we used [nanodiamond] probes here…", body).unwrap();
        assert_eq!(&body[at..at + "nanodiamond".len()], "nanodiamond");
    }

    /// Every prefix `normalize_doi` strips has to match on the stored side too.
    /// Normalizing only the incoming DOI lets the un-stripped forms sit in the
    /// table as permanent duplicates.
    #[test]
    fn find_paper_by_doi_matches_every_normalized_prefix() {
        for stored in [
            "https://doi.org/10.1038/nature12373",
            "http://doi.org/10.1038/nature12373",
            "http://dx.doi.org/10.1038/nature12373",
            "https://dx.doi.org/10.1038/nature12373",
            "doi:10.1038/nature12373",
            "10.1038/NATURE12373",
        ] {
            let store = test_store();
            let mut paper = Paper::new("stored form".to_string());
            paper.doi = Some(stored.to_string());
            store.insert_paper(&paper).unwrap();
            assert!(
                store
                    .find_paper_by_doi("10.1038/nature12373")
                    .unwrap()
                    .is_some(),
                "stored form {stored} did not match a normalized probe"
            );
        }
    }

    /// Scoping to one paper must happen in the query. Filtering a whole-library
    /// result set afterwards loses the target paper's matches whenever other
    /// papers fill the limit first.
    #[test]
    fn body_evidence_scoped_to_paper_survives_a_crowded_library() {
        let store = test_store();
        // Many papers match the same term; the one we want is inserted last so
        // a whole-library search with a small limit would not reach it.
        for i in 0..10 {
            let noise = Paper::new(format!("noise {i}"));
            store.insert_paper(&noise).unwrap();
            store
                .set_paper_body(&noise.id, "## Intro\nshared keyword here")
                .unwrap();
        }
        let target = Paper::new("target".into());
        store.insert_paper(&target).unwrap();
        store
            .set_paper_body(&target.id, "## Methods\nshared keyword here too")
            .unwrap();

        let scoped = store
            .search_body_evidence("keyword", Some(&target.id), 3)
            .unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].paper_id, target.id);
        assert_eq!(scoped[0].anchor.section.as_deref(), Some("Methods"));

        // Unscoped still searches everything.
        let all = store.search_body_evidence("keyword", None, 20).unwrap();
        assert_eq!(all.len(), 11);
    }

    /// A limit smaller than the match count must keep the *best* matches, not
    /// whichever rows SQLite happened to emit first. Without `ORDER BY rank`
    /// the survivors are unspecified row order and the strongest evidence can
    /// be dropped silently.
    #[test]
    fn body_evidence_returns_the_best_matches_under_a_limit() {
        let store = test_store();
        // Weak matches are inserted first so unordered row order would favour
        // them; the dense match is inserted last.
        for i in 0..8 {
            let weak = Paper::new(format!("weak {i}"));
            store.insert_paper(&weak).unwrap();
            store
                .set_paper_body(&weak.id, "## Intro\nphotonic mentioned once here")
                .unwrap();
        }
        let strong = Paper::new("strong".into());
        store.insert_paper(&strong).unwrap();
        store
            .set_paper_body(
                &strong.id,
                "## Methods\nphotonic photonic photonic photonic photonic lattice",
            )
            .unwrap();

        let top = store.search_body_evidence("photonic", None, 1).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(
            top[0].paper_id, strong.id,
            "limit kept an arbitrary row instead of the best-ranked match"
        );
    }

    /// The exact scenario an audit reproduced: a term that also occurs inside
    /// an earlier heading. Anchoring on the term's first occurrence reported
    /// the wrong page and a section name truncated mid-word ("Introduct").
    #[test]
    fn body_evidence_anchors_the_matched_occurrence_not_the_first() {
        let store = test_store();
        let paper = Paper::new("ion beam".into());
        store.insert_paper(&paper).unwrap();
        store
            .set_paper_body(
                &paper.id,
                "<!-- page 1 -->\n## Introduction\nbackground material\n<!-- page 3 -->\n## Results\nthe ion beam produced clean output\n",
            )
            .unwrap();

        let hits = store.search_body_evidence("ion beam", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        // "ion" also lives inside "Introduction" on page 1; the trigram
        // tokenizer matches inside words, so this is a real collision.
        assert_eq!(hits[0].anchor.section.as_deref(), Some("Results"));
        assert_eq!(hits[0].anchor.page, Some(3));
    }

    /// DOI matching is normalization-insensitive on both sides: papers stored
    /// by an earlier ingest keep whatever form upstream sent, and must still
    /// dedupe against a normalized DOI arriving from an import.
    #[test]
    fn find_paper_by_doi_matches_across_stored_forms() {
        let store = test_store();
        let mut raw = Paper::new("stored raw".into());
        raw.doi = Some("https://doi.org/10.1038/NATURE12373".into());
        store.insert_paper(&raw).unwrap();

        for probe in [
            "10.1038/nature12373",
            "10.1038/NATURE12373",
            "https://doi.org/10.1038/nature12373",
            "  doi:10.1038/Nature12373 ",
        ] {
            assert_eq!(
                store.find_paper_by_doi(probe).unwrap().map(|p| p.id),
                Some(raw.id.clone()),
                "probe {probe:?} should match the stored paper"
            );
        }
        assert!(store.find_paper_by_doi("10.1/other").unwrap().is_none());
    }
}
