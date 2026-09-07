//! MCP stdio server — exposes research-agent's capabilities as MCP tools
//! (`research serve`). Inverts control: instead of a human driving the `research`
//! CLI, an LLM agent discovers and drives research-agent via these tools
//! (ingest → query → gaps → report). The CLI remains a secondary interface for
//! terminals/CI.
//!
//! Gated behind the `mcp` cargo feature so `--no-default-features` builds stay
//! CLI-only. rmcp = official Rust MCP SDK; schemars derives each tool's
//! `inputSchema`.

pub mod params;
pub mod server;

mod guard;
