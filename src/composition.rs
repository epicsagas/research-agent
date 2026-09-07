//! Composition root: the single place where concrete adapters are wired onto
//! the application ports. All entry points (CLI and MCP server) build
//! their dependencies through this module so wiring exists exactly once.

use std::path::{Path, PathBuf};

use crate::adapters::llm_research_engine::LlmResearchEngine;
use crate::adapters::sqlite_store::SqliteStore;
use crate::config::{Config, default_config_path, default_db_path};
use crate::error::{ResearchError, Result};

/// Open the workspace SQLite store at `db_path`.
pub fn open_store(db_path: &Path) -> Result<SqliteStore> {
    SqliteStore::open(db_path)
}

/// The database path: the global `--db` flag wins, otherwise the default
/// under `~/.research`.
pub fn resolve_db(opt: &Option<PathBuf>) -> PathBuf {
    opt.clone().unwrap_or_else(default_db_path)
}

/// Load `config.toml`, materializing the default file when absent.
pub fn load_config() -> Result<Config> {
    Config::load(&default_config_path()).map_err(|e| ResearchError::Config(e.to_string()))
}

/// Build the LLM research engine from `config.toml` `[llm]`. A missing
/// `[llm]` section yields the default (provider-configured) engine; a missing
/// API key is only warned about here — the engine resolves it at call time.
pub fn make_llm_engine(store: SqliteStore) -> Result<LlmResearchEngine> {
    let config = load_config()?;
    if let Some(llm_cfg) = config.llm {
        if llm_cfg.resolve_api_key().is_none() {
            tracing::warn!(
                api_key_env = %llm_cfg.api_key_env,
                "[llm] api_key_env not set — LLM calls will fail"
            );
        }
        use llm_kernel::llm::ModelConfig;
        let model_config = ModelConfig {
            provider: llm_cfg.provider,
            model: llm_cfg.model,
            api_key_env: llm_cfg.api_key_env,
            base_url: llm_cfg.base_url,
            ..ModelConfig::default()
        };
        return Ok(LlmResearchEngine::with_config(
            Box::new(store),
            model_config,
        ));
    }
    Ok(LlmResearchEngine::new(Box::new(store)))
}
