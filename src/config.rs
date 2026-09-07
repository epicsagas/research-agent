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
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
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
}
