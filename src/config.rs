use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: String,
    /// OpenAI-compatible base URL (e.g. https://api.openai.com/v1). Optional —
    /// providers with a known default can leave it unset.
    #[serde(default)]
    pub base_url: Option<String>,
}

impl LlmConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        std::env::var(&self.api_key_env).ok()
    }
}

/// Hybrid-search configuration. Absent section = local ONNX defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// "local" (bundled ONNX, default) or "openai" (remote, BYOK).
    #[serde(default = "search_provider_default")]
    pub provider: String,
    /// Local model id (llm-kernel `EmbeddingModel`). Small by default so
    /// low-VRAM machines and CPU-only hosts stay fast.
    #[serde(default = "search_model_default")]
    pub model: String,
    /// Env var holding the OpenAI key when provider = "openai".
    #[serde(default = "search_key_env_default")]
    pub openai_api_key_env: String,
}

fn search_provider_default() -> String {
    "local".into()
}
fn search_model_default() -> String {
    "BGESmallENV15".into()
}
fn search_key_env_default() -> String {
    "OPENAI_API_KEY".into()
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: search_provider_default(),
            model: search_model_default(),
            openai_api_key_env: search_key_env_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_path: PathBuf,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    /// Hybrid (lexical + vector) search config. Optional; missing section uses
    /// local defaults.
    #[serde(default)]
    pub search: Option<SearchConfig>,
}

/// Returns `~/.research` on all platforms (Windows: `C:\Users\<user>\.research`).
pub fn research_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".research")
}

pub fn default_db_path() -> PathBuf {
    research_dir().join("research.db")
}

pub fn default_config_path() -> PathBuf {
    research_dir().join("config.toml")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_path: default_db_path(),
            llm: None,
            search: None,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save(path)?;
            Ok(config)
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, config_template(self))?;
        Ok(())
    }
}

/// Quote a string as a TOML value (keeps user paths with quotes/backslashes
/// valid).
fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Render the config as a fully documented template: `database_path` is
/// always live, and every key of the optional `[llm]` / `[search]` sections
/// appears — set values as live TOML, unset ones commented out with their
/// defaults — so the file alone shows everything that is configurable.
pub fn config_template(cfg: &Config) -> String {
    let mut out = String::new();
    out.push_str("# research-agent configuration. Re-run `research init` to edit interactively.\n");
    out.push_str(&format!(
        "\ndatabase_path = {}\n",
        toml_str(&cfg.database_path.to_string_lossy())
    ));

    out.push_str("\n# LLM used for gap analysis and report generation. Without this section\n");
    out.push_str("# those commands return placeholder text instead of real analysis.\n");
    match &cfg.llm {
        Some(llm) => {
            out.push_str("[llm]\n");
            out.push_str("# \"anthropic\" or an OpenAI-compatible provider id\n");
            out.push_str(&format!("provider = {}\n", toml_str(&llm.provider)));
            out.push_str(&format!("model = {}\n", toml_str(&llm.model)));
            out.push_str("# env var holding the API key; the key itself is never stored here\n");
            out.push_str(&format!("api_key_env = {}\n", toml_str(&llm.api_key_env)));
            match &llm.base_url {
                Some(url) => out.push_str(&format!("base_url = {}\n", toml_str(url))),
                None => out.push_str("# base_url = \"https://api.example.com/v1\"  # for OpenAI-compatible endpoints without a known default\n"),
            }
        }
        None => {
            for line in [
                "# [llm]",
                "# provider = \"anthropic\"  # \"anthropic\" or an OpenAI-compatible provider id (openai, deepseek, openrouter, ollama, ...)",
                "# model = \"claude-sonnet-4-6\"",
                "# api_key_env = \"ANTHROPIC_API_KEY\"  # env var holding the API key; the key itself is never stored here",
                "# base_url = \"https://api.example.com/v1\"  # for OpenAI-compatible endpoints without a known default",
            ] {
                out.push_str(line);
                out.push('\n');
            }
        }
    }

    out.push_str("\n# Hybrid search: FTS5 lexical hits fused with embedding hits. Missing\n");
    out.push_str("# section = local ONNX defaults. The local model downloads to\n");
    out.push_str("# ~/.research/models on first use.\n");
    match &cfg.search {
        Some(search) => {
            out.push_str("[search]\n");
            out.push_str("# \"local\" (bundled ONNX) or \"openai\" (BYOK remote embeddings)\n");
            out.push_str(&format!("provider = {}\n", toml_str(&search.provider)));
            out.push_str("# local model id (llm-kernel EmbeddingModel, e.g. BGESmallENV15)\n");
            out.push_str(&format!("model = {}\n", toml_str(&search.model)));
            out.push_str("# env var holding the OpenAI key when provider = \"openai\"\n");
            out.push_str(&format!(
                "openai_api_key_env = {}\n",
                toml_str(&search.openai_api_key_env)
            ));
        }
        None => {
            for line in [
                "# [search]",
                "# provider = \"local\"  # \"local\" (bundled ONNX) or \"openai\" (BYOK remote embeddings)",
                "# model = \"BGESmallENV15\"  # local model id (llm-kernel EmbeddingModel)",
                "# openai_api_key_env = \"OPENAI_API_KEY\"  # env var holding the OpenAI key when provider = \"openai\"",
            ] {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_db_is_under_home_research() {
        let path = default_db_path();
        assert!(
            path.ends_with(".research/research.db") || path.to_string_lossy().contains(".research")
        );
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            database_path: PathBuf::from("/tmp/test.db"),
            llm: None,
            search: None,
        };
        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.database_path, config.database_path);
    }

    #[test]
    fn load_creates_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.toml");
        let config = Config::load(&path).unwrap();
        assert!(config.database_path.ends_with("research.db"));
        assert!(path.exists());
    }

    #[test]
    fn llm_config_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            database_path: PathBuf::from("research.db"),
            llm: Some(LlmConfig {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                api_key_env: "ANTHROPIC_API_KEY".into(),
                base_url: None,
            }),
            search: None,
        };
        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        let llm = loaded.llm.unwrap();
        assert_eq!(llm.provider, "anthropic");
        assert_eq!(llm.model, "claude-sonnet-4-6");
        assert_eq!(llm.api_key_env, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn config_without_llm_section_loads_ok() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "database_path = \"research.db\"\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.llm.is_none());
    }

    #[test]
    fn malformed_config_errors_instead_of_falling_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "database_path = \n [llm\n").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn default_template_roundtrips_with_optional_keys_commented() {
        let text = config_template(&Config::default());
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.database_path, default_db_path());
        assert!(parsed.llm.is_none());
        assert!(parsed.search.is_none());
        // The template exists to document every configurable key, so unset
        // sections must still be visible as comments.
        for key in [
            "provider",
            "model",
            "api_key_env",
            "base_url",
            "openai_api_key_env",
        ] {
            assert!(
                text.contains(&format!("# {key}")),
                "missing commented key: {key}"
            );
        }
    }

    #[test]
    fn set_sections_render_live_and_parse_back() {
        let config = Config {
            database_path: PathBuf::from("/tmp/r.db"),
            llm: Some(LlmConfig {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                api_key_env: "ANTHROPIC_API_KEY".into(),
                base_url: None,
            }),
            search: Some(SearchConfig {
                provider: "openai".into(),
                ..SearchConfig::default()
            }),
        };
        let text = config_template(&config);
        let parsed: Config = toml::from_str(&text).unwrap();
        let llm = parsed.llm.unwrap();
        assert_eq!(llm.provider, "anthropic");
        assert_eq!(llm.api_key_env, "ANTHROPIC_API_KEY");
        let search = parsed.search.unwrap();
        assert_eq!(search.provider, "openai");
        assert_eq!(search.model, "BGESmallENV15");
        // Values that are set render live, not as comments.
        assert!(text.contains("provider = \"anthropic\""));
        assert!(text.contains("provider = \"openai\""));
    }

    #[test]
    fn template_quotes_paths_safely() {
        let config = Config {
            database_path: PathBuf::from("/home/u/space dir/r.db"),
            ..Config::default()
        };
        let text = config_template(&config);
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed.database_path,
            PathBuf::from("/home/u/space dir/r.db")
        );
    }
}
