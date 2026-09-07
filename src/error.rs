use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResearchError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("duplicate: {0}")]
    Duplicate(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("source error: {0}")]
    Source(String),
}

pub type Result<T> = std::result::Result<T, ResearchError>;
