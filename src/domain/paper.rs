use crate::error::{ResearchError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaperStatus {
    Discovered,
    AbstractRead,
    Skimmed,
    Read,
    DeepRead,
}

impl PaperStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::AbstractRead => "abstract_read",
            Self::Skimmed => "skimmed",
            Self::Read => "read",
            Self::DeepRead => "deep_read",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "abstract_read" => Self::AbstractRead,
            "skimmed" => Self::Skimmed,
            "read" => Self::Read,
            "deep_read" => Self::DeepRead,
            _ => Self::Discovered,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    Unread,
    Queued,
    InProgress,
    Completed,
    Abandoned,
}

impl ReadingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "abandoned" => Self::Abandoned,
            _ => Self::Unread,
        }
    }
}

/// User rating on the 1–5 scale. The constructor and the serde path both
/// enforce bounds, so a `Rating` value is always valid — callers and
/// deserialized data cannot bypass validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Rating(u8);

impl<'de> Deserialize<'de> for Rating {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        Rating::new(v).map_err(serde::de::Error::custom)
    }
}

impl Rating {
    /// Create a rating, rejecting anything outside 1..=5.
    pub fn new(value: u8) -> Result<Self> {
        if (1..=5).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ResearchError::Validation(format!(
                "rating must be between 1 and 5, got {value}"
            )))
        }
    }

    /// The underlying 1–5 value.
    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub abstract_text: String,
    pub year: Option<u32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub s2_id: Option<String>,
    /// OpenAlex work id (`W…`), set by the OpenAlex source.
    #[serde(default)]
    pub openalex_id: Option<String>,
    pub url: Option<String>,
    pub pdf_path: Option<String>,
    pub status: PaperStatus,
    pub notes: String,
    pub tags: Vec<String>,
    pub relevance_score: f32,
    pub reading_status: ReadingStatus,
    /// User rating 1–5, None if not yet rated.
    pub rating: Option<Rating>,
    /// Search-only keywords generated from the title and abstract (by the
    /// configured LLM, or by a host agent through `research enrich`).
    /// Indexed by FTS5 so a paraphrased query can reach a paper whose abstract
    /// never uses the query's wording. Deliberately separate from `tags`,
    /// which are the user's own and get pushed to Zotero.
    #[serde(default)]
    pub keywords: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Paper {
    pub fn new(title: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            authors: Vec::new(),
            abstract_text: String::new(),
            year: None,
            venue: None,
            doi: None,
            arxiv_id: None,
            s2_id: None,
            openalex_id: None,
            url: None,
            pdf_path: None,
            status: PaperStatus::Discovered,
            notes: String::new(),
            tags: Vec::new(),
            relevance_score: 0.5,
            reading_status: ReadingStatus::Unread,
            rating: None,
            keywords: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Normalize a title into the form used for identity matching: punctuation
/// dropped (letters and digits kept, so hyphenated words and CJK titles
/// survive), lowercased, whitespace runs collapsed to one space. Dropping
/// punctuation catches the same paper exported with a trailing period by one
/// source and not the other. This is the fallback key for papers that carry
/// neither a DOI nor a pdf_path, so the stored side must be compared through
/// the same function (the store cannot express this collapse in SQL, which
/// is why the lookup scans and compares here). Two genuinely different
/// papers sharing one normalized title collapse to one row — accepted for a
/// personal library.
pub fn normalize_title(title: &str) -> String {
    let stripped: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_status_roundtrip() {
        let statuses = [
            PaperStatus::Discovered,
            PaperStatus::AbstractRead,
            PaperStatus::Skimmed,
            PaperStatus::Read,
            PaperStatus::DeepRead,
        ];
        for s in &statuses {
            assert_eq!(PaperStatus::from_str_lossy(s.as_str()), *s);
        }
    }

    #[test]
    fn reading_status_roundtrip() {
        let statuses = [
            ReadingStatus::Unread,
            ReadingStatus::Queued,
            ReadingStatus::InProgress,
            ReadingStatus::Completed,
            ReadingStatus::Abandoned,
        ];
        for s in &statuses {
            assert_eq!(ReadingStatus::from_str_lossy(s.as_str()), *s);
        }
    }

    #[test]
    fn rating_accepts_valid_range() {
        for v in [1u8, 2, 3, 4, 5] {
            assert_eq!(Rating::new(v).unwrap().get(), v);
        }
    }

    #[test]
    fn rating_rejects_out_of_range() {
        assert!(Rating::new(0).is_err());
        assert!(Rating::new(6).is_err());
        assert!(Rating::new(255).is_err());
    }

    #[test]
    fn rating_deserialize_enforces_bounds() {
        // serde must not bypass Rating::new — out-of-range values error.
        assert!(serde_json::from_str::<Rating>("0").is_err());
        assert!(serde_json::from_str::<Rating>("6").is_err());
        assert!(serde_json::from_str::<Rating>("99").is_err());
        assert_eq!(serde_json::from_str::<Rating>("3").unwrap().get(), 3);
    }

    #[test]
    fn paper_new_generates_id_and_timestamps() {
        let p = Paper::new("Test Paper".into());
        assert_eq!(p.title, "Test Paper");
        assert!(!p.id.is_empty());
        assert!(!p.created_at.is_empty());
        assert_eq!(p.status, PaperStatus::Discovered);
        assert_eq!(p.reading_status, ReadingStatus::Unread);
    }

    #[test]
    fn paper_serialization_roundtrip() {
        let p = Paper::new("Serialization Test".into());
        let json = serde_json::to_string(&p).unwrap();
        let back: Paper = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, p.title);
        assert_eq!(back.id, p.id);
    }
}
