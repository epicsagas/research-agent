//! MCP tool parameter structs.
//!
//! Each derives `serde::Deserialize + schemars::JsonSchema` (required by rmcp so
//! the tool's `inputSchema` is derived automatically). Only **primitive** types
//! are used — research-agent domain types (which derive `Serialize` but **not**
//! `JsonSchema`) are returned from tools as opaque `serde_json::Value` instead.
//!
//! Tools that take no parameters (`init`, `index_rebuild`, `state`,
//! `topics_list`) declare no `Parameters` at all, mirroring BYOH's `genre_list`.

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IngestParams {
    /// Search query (required for arxiv/s2/all; ignored for pdf).
    #[serde(default)]
    pub query: Option<String>,
    /// Source: "arxiv" | "s2" | "all" | "pdf" (default "all").
    #[serde(default = "default_source")]
    pub source: String,
    /// Maximum papers to fetch for arxiv/s2 (default 10).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Path to a PDF file or directory (required for source=pdf).
    #[serde(default)]
    pub path: Option<String>,
    /// Optional topic id to link the ingested papers to.
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryPapersParams {
    pub query: String,
    /// Maximum results (default 20).
    #[serde(default = "default_query_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeGapsParams {
    /// Topic id to analyze for knowledge gaps.
    pub topic: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListGapsParams {
    /// Optional topic id filter.
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateReportParams {
    /// Report title (default "Research Report").
    #[serde(default = "default_title")]
    pub title: String,
    /// Comma-separated topic ids.
    pub topic: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicAddParams {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Parent topic id (creates a sub-topic).
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateReadParams {
    /// Paper id.
    pub id: String,
    /// New status: unread | queued | in_progress | completed | abandoned.
    #[serde(default)]
    pub status: Option<String>,
    /// Rating 1–5.
    #[serde(default)]
    pub rating: Option<u8>,
}

fn default_source() -> String {
    "all".to_string()
}
fn default_limit() -> usize {
    10
}
fn default_query_limit() -> usize {
    20
}
fn default_title() -> String {
    "Research Report".to_string()
}
