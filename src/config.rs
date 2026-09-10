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

/// `[dashboard]` section: how the web dashboard is served. Binding to a
/// non-loopback host requires a token — without one every request would be
/// readable (and writable) by the whole network.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Bind address: "127.0.0.1" (default) or "0.0.0.0" for LAN access.
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// Shared secret. Requests must present it via `Authorization: Bearer`
    /// or the `dashboard_token` cookie.
    #[serde(default)]
    pub token: Option<String>,
}

impl DashboardConfig {
    pub fn host_or_default(&self) -> &str {
        self.host.as_deref().unwrap_or("127.0.0.1")
    }

    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or(7777)
    }

    pub fn is_loopback(&self) -> bool {
        self.host_or_default().starts_with("127.0.0.1") || self.host_or_default() == "localhost"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_path: PathBuf,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    #[serde(default)]
    pub dashboard: DashboardConfig,
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
            dashboard: DashboardConfig::default(),
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
/// always live, and every key of the optional `[llm]` section
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

    out.push_str("\n# Web dashboard (`research dashboard`). Loopback-only and port 7777 by\n");
    out.push_str("# default; a non-loopback host requires a token.\n");
    let d = &cfg.dashboard;
    if d.host.is_some() || d.port.is_some() || d.token.is_some() {
        out.push_str("[dashboard]\n");
        if let Some(host) = &d.host {
            out.push_str(&format!("host = {}\n", toml_str(host)));
        }
        if let Some(port) = d.port {
            out.push_str(&format!("port = {port}\n"));
        }
        if let Some(token) = &d.token {
            out.push_str(&format!("token = {}\n", toml_str(token)));
        }
    } else {
        for line in [
            "# [dashboard]",
            "# host = \"127.0.0.1\"  # bind address; use 0.0.0.0 for LAN access (token required)",
            "# port = 7777",
            "# token = \"...\"  # shared secret; sent as Authorization: Bearer or dashboard_token cookie",
        ] {
            out.push_str(line);
            out.push('\n');
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
            dashboard: Default::default(),
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
            dashboard: Default::default(),
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
        // The template exists to document every configurable key, so unset
        // sections must still be visible as comments.
        for key in [
            "provider",
            "model",
            "api_key_env",
            "base_url",
            "host",
            "port",
            "token",
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
            dashboard: Default::default(),
        };
        let text = config_template(&config);
        let parsed: Config = toml::from_str(&text).unwrap();
        let llm = parsed.llm.unwrap();
        assert_eq!(llm.provider, "anthropic");
        assert_eq!(llm.api_key_env, "ANTHROPIC_API_KEY");
        // Values that are set render live, not as comments.
        assert!(text.contains("provider = \"anthropic\""));
    }

    #[test]
    fn legacy_search_section_is_ignored_not_fatal() {
        // Configs written before embeddings were removed still carry a
        // [search] section. Parsing must skip it rather than fail — otherwise
        // every existing install breaks on upgrade.
        let text = r#"
database_path = "/tmp/legacy.db"

[search]
provider = "local"
model = "BGESmallENV15"
openai_api_key_env = "OPENAI_API_KEY"
embed_batch_size = 16
embed_memory_budget_mb = 1024
"#;
        let parsed: Config = toml::from_str(text).unwrap();
        assert_eq!(parsed.database_path, PathBuf::from("/tmp/legacy.db"));
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
