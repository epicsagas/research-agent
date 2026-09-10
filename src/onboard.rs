//! First-run onboarding for `research init`: walks through the settings that
//! matter (database path, LLM provider with its env-var key name), then
//! writes a fully documented
//! `config.toml`. Every prompt shows the current value as its default, so
//! re-running init updates instead of resetting.

use std::path::PathBuf;

use anyhow::{Result, bail};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};

use crate::config::LlmConfig;

/// One curated LLM provider entry. Only providers that map cleanly onto the
/// two client paths in `llm_research_engine` ("anthropic" -> AnthropicClient,
/// everything else -> OpenAIClient with a base_url) are listed; anything else
/// fits the custom entry.
struct LlmProvider {
    label: &'static str,
    /// Value written to `[llm].provider`.
    provider: &'static str,
    /// Host base for discovery probes; the OpenAI-compatible API base is this
    /// plus `/v1` (empty means the client's own default).
    api_host: &'static str,
    key_env: &'static str,
    default_model: &'static str,
    /// Live model discovery via a local API (ollama, LM Studio).
    local: bool,
}

const LLM_PROVIDERS: &[LlmProvider] = &[
    LlmProvider {
        label: "Anthropic",
        provider: "anthropic",
        api_host: "",
        key_env: "ANTHROPIC_API_KEY",
        default_model: "claude-sonnet-4-6",
        local: false,
    },
    LlmProvider {
        label: "OpenAI",
        provider: "openai",
        api_host: "",
        key_env: "OPENAI_API_KEY",
        default_model: "gpt-4o",
        local: false,
    },
    LlmProvider {
        label: "DeepSeek",
        provider: "deepseek",
        api_host: "https://api.deepseek.com",
        key_env: "DEEPSEEK_API_KEY",
        default_model: "deepseek-chat",
        local: false,
    },
    LlmProvider {
        label: "OpenRouter",
        provider: "openrouter",
        api_host: "https://openrouter.ai/api",
        key_env: "OPENROUTER_API_KEY",
        default_model: "openai/gpt-4o-mini",
        local: false,
    },
    LlmProvider {
        label: "Ollama (local, no key needed)",
        provider: "ollama",
        api_host: "http://localhost:11434",
        key_env: "OLLAMA_API_KEY",
        default_model: "llama3.2",
        local: true,
    },
    LlmProvider {
        label: "LM Studio (local, no key needed)",
        provider: "lmstudio",
        api_host: "http://localhost:1234",
        key_env: "LMSTUDIO_API_KEY",
        default_model: "local-model",
        local: true,
    },
];

pub fn run(
    db_path: PathBuf,
    existing: Option<crate::config::Config>,
) -> Result<crate::config::Config> {
    let theme = ColorfulTheme::default();
    println!("Welcome to research-agent. A few questions to set things up;");
    println!("press Enter to accept the (default) on any step.\n");

    // 1. Database path.
    let db_default = existing
        .as_ref()
        .map(|c| c.database_path.clone())
        .unwrap_or(db_path);
    let db: String = Input::with_theme(&theme)
        .with_prompt("Where should the library live?")
        .default(db_default.to_string_lossy().into_owned())
        .interact()
        .map_err(canceled)?;
    let db_path = PathBuf::from(expand_home(&db));

    // 2. LLM provider (gap analysis / report generation).
    let mut llm = existing.as_ref().and_then(|c| c.llm.clone());
    let mut items: Vec<&str> = vec!["Skip for now (configure later in config.toml)"];
    items.extend(LLM_PROVIDERS.iter().map(|p| p.label));
    items.push("Custom (OpenAI-compatible base URL)");
    let pick = Select::with_theme(&theme)
        .with_prompt("LLM provider for gap analysis and reports")
        .default(match &llm {
            Some(l) => LLM_PROVIDERS
                .iter()
                .position(|p| p.provider == l.provider)
                .map_or(0, |i| i + 1),
            None => 0,
        })
        .items(&items)
        .interact()
        .map_err(canceled)?;
    if pick > 0 {
        llm = Some(prompt_llm(&theme, pick, llm.as_ref())?);
    }

    // 3. Summary + write.
    println!("\nSummary");
    println!("  database: {}", db_path.display());
    match &llm {
        Some(l) => println!(
            "  llm:      {} / {} (key from ${}; base_url: {})",
            l.provider,
            l.model,
            l.api_key_env,
            l.base_url.as_deref().unwrap_or("provider default")
        ),
        None => println!("  llm:      not configured (gaps/report return placeholders)"),
    }
    if !Confirm::with_theme(&theme)
        .with_prompt("Write config.toml with these values?")
        .default(true)
        .interact()
        .map_err(canceled)?
    {
        bail!("Aborted — nothing was written.");
    }

    Ok(crate::config::Config {
        database_path: db_path,
        llm,
        dashboard: Default::default(),
    })
}

fn prompt_llm(
    theme: &ColorfulTheme,
    pick: usize,
    existing: Option<&LlmConfig>,
) -> Result<LlmConfig> {
    // The custom entry asks for everything; curated entries fill from the
    // table (with the existing config as defaults when the provider matches).
    if pick > LLM_PROVIDERS.len() {
        let provider: String = Input::with_theme(theme)
            .with_prompt("Provider id (written to config, e.g. \"openai\")")
            .default("openai".into())
            .interact()
            .map_err(canceled)?;
        let base: String = Input::with_theme(theme)
            .with_prompt("OpenAI-compatible base URL (e.g. https://host/v1)")
            .interact()
            .map_err(canceled)?;
        let api_key_env: String = Input::with_theme(theme)
            .with_prompt("Env var holding the API key")
            .default("OPENAI_API_KEY".into())
            .interact()
            .map_err(canceled)?;
        check_env(&api_key_env);
        let model: String = Input::with_theme(theme)
            .with_prompt("Model id")
            .interact()
            .map_err(canceled)?;
        return Ok(LlmConfig {
            provider,
            model,
            api_key_env,
            base_url: Some(base),
        });
    }

    let p = &LLM_PROVIDERS[pick - 1];
    let same = existing.filter(|l| l.provider == p.provider);
    let key_default = match same {
        Some(l) => l.api_key_env.clone(),
        None => p.key_env.to_string(),
    };
    let model = match same {
        Some(l) => l.model.clone(),
        None => {
            let fallback = p.default_model.to_string();
            if !p.local {
                Input::with_theme(theme)
                    .with_prompt("Model id")
                    .default(fallback)
                    .interact()
                    .map_err(canceled)?
            } else {
                // Local servers (ollama, LM Studio) can name their loaded
                // models; a 2s probe beats guessing an id that isn't there.
                let models = if p.provider == "ollama" {
                    llm_kernel::discovery::fetch_ollama_models(p.api_host)
                } else {
                    llm_kernel::discovery::fetch_openai_compatible_models(p.api_host)
                };
                match models {
                    Ok(list) if !list.is_empty() => {
                        let mut items: Vec<&str> = vec!["Other (type it)"];
                        items.extend(list.iter().map(|s| s.as_str()));
                        let pick = Select::with_theme(theme)
                            .with_prompt("Model")
                            .items(&items)
                            .interact()
                            .map_err(canceled)?;
                        if pick > 0 {
                            list[pick - 1].clone()
                        } else {
                            Input::<String>::with_theme(theme)
                                .with_prompt("Model id")
                                .interact()
                                .map_err(canceled)?
                        }
                    }
                    _ => Input::with_theme(theme)
                        .with_prompt("Model id")
                        .default(fallback)
                        .interact()
                        .map_err(canceled)?,
                }
            }
        }
    };
    let api_key_env = Input::with_theme(theme)
        .with_prompt("Env var holding the API key")
        .default(key_default)
        .interact()
        .map_err(canceled)?;
    check_env(&api_key_env);
    // "anthropic" routes to AnthropicClient (its endpoint is built in) and
    // "openai" to OpenAIClient's default; the rest are OpenAI-compatible
    // endpoints that need an explicit /v1 base.
    let base_url = match p.provider {
        "anthropic" | "openai" => None,
        other => Some(format!("{}/v1", provider_base(other))),
    };
    Ok(LlmConfig {
        provider: p.provider.to_string(),
        model,
        api_key_env,
        base_url,
    })
}

/// Known OpenAI-compatible API hosts for the curated ids (mirrors
/// LLM_PROVIDERS). Custom ids set base_url at input time instead.
fn provider_base(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "openrouter" => "https://openrouter.ai/api",
        "ollama" => "http://localhost:11434",
        "lmstudio" => "http://localhost:1234",
        _ => "",
    }
}

fn check_env(name: &str) {
    if name.is_empty() {
        return;
    }
    match std::env::var(name) {
        Ok(_) => println!("  ✓ {name} is set in this shell"),
        Err(_) => {
            println!("  ! {name} is not set — export {name}=... before running gaps or report")
        }
    }
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// Map dialoguer's interrupt to a clean message instead of an io error dump.
fn canceled(e: dialoguer::Error) -> anyhow::Error {
    anyhow::anyhow!("Onboarding canceled: {e}")
}
