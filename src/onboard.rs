//! First-run onboarding for `research init`: walks through the settings that
//! matter (database path, LLM provider with its env-var key name, embedding
//! model with an optional immediate download), then writes a fully documented
//! `config.toml`. Every prompt shows the current value as its default, so
//! re-running init updates instead of resetting.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use llm_kernel::embedding::{EmbeddingModel, EmbeddingProvider, FastembedProvider};

use crate::config::{LlmConfig, SearchConfig};

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

    // 3. Embeddings (hybrid search).
    let mut search = existing.as_ref().and_then(|c| c.search.clone());
    let search_pick = Select::with_theme(&theme)
        .with_prompt("Embedding model for hybrid search")
        .default(0)
        .items([
            "Local model (bundled ONNX, downloads to ~/.research/models)",
            "OpenAI embeddings (BYOK)",
            "Disable hybrid search (lexical only)",
        ])
        .interact()
        .map_err(canceled)?;
    match search_pick {
        0 => search = Some(prompt_local_embed(&theme, search.as_ref())?),
        1 => search = Some(prompt_openai_embed(&theme, search.as_ref())?),
        _ => search = None,
    }

    // 4. Summary + write.
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
    match &search {
        Some(s) => println!(
            "  search:   {} / {}",
            s.provider,
            if s.provider == "openai" {
                s.openai_api_key_env.as_str()
            } else {
                s.model.as_str()
            }
        ),
        None => println!("  search:   lexical only"),
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
        search,
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

/// Curated local embedding models for academic-paper hybrid search, best
/// value first. Everything image-only (CLIP), code-specific, or dominated by
/// a sibling in this list stays out; the full catalog remains one keystroke
/// away via "Show all models…".
const CURATED: &[(EmbeddingModel, &str)] = &[
    (EmbeddingModel::BGESmallENV15, "best value"),
    (EmbeddingModel::BGESmallENV15Q, "low-mem"),
    (EmbeddingModel::AllMiniLML6V2Q, "low-mem"),
    (EmbeddingModel::SnowflakeArcticEmbedSQ, "low-mem"),
    (EmbeddingModel::SnowflakeArcticEmbedS, "balanced"),
    (EmbeddingModel::EmbeddingGemma300M, "multilingual"),
    (EmbeddingModel::MultilingualE5Small, "multilingual"),
    (EmbeddingModel::BGESmallZHV15, "multilingual (zh)"),
    (EmbeddingModel::GTEBaseENV15Q, "balanced"),
    (EmbeddingModel::SnowflakeArcticEmbedMLong, "long context"),
    (EmbeddingModel::BGEM3, "multilingual + long"),
    (EmbeddingModel::BGELargeENV15, "max quality"),
];

/// Rough runtime peak for an ONNX embedding model: resident weights plus
/// activation headroom, with a floor for the ONNX session itself.
fn ram_need_mb(model: EmbeddingModel) -> usize {
    model.size_mb() * 2 + 256
}

/// Models the short list this machine can run without swapping: half of
/// total RAM keeps the OS, the DB, and the embedding arena apart. `None`
/// budget (probe failed) means no filtering. A tiny machine that fits
/// nothing still gets the smallest quantized model so the menu is never
/// empty.
/// ponytail: RAM as the only hardware signal — VRAM probing is not portable;
/// revisit if DirectML VRAM matters someday.
fn curated_within_budget(budget_mb: Option<usize>) -> Vec<EmbeddingModel> {
    let in_budget = |m: &EmbeddingModel| budget_mb.is_none_or(|b| ram_need_mb(*m) <= b);
    let mut fits: Vec<EmbeddingModel> = CURATED
        .iter()
        .filter(|(m, _)| in_budget(m))
        .map(|(m, _)| *m)
        .collect();
    if fits.is_empty() {
        fits.push(EmbeddingModel::BGESmallENV15Q);
    }
    fits
}

fn prompt_local_embed(
    theme: &ColorfulTheme,
    existing: Option<&SearchConfig>,
) -> Result<SearchConfig> {
    let default_model = existing
        .filter(|s| s.provider != "openai")
        .map(|s| s.model.clone())
        .unwrap_or_else(|| "BGESmallENV15".to_string());
    let cached = |m: EmbeddingModel| {
        llm_kernel::embedding::lazy::is_model_cached(
            m,
            &crate::config::research_dir().join("models"),
        )
    };

    // Short list sized to this machine, with the full catalog as the last
    // entry for anyone who wants the rest of the zoo.
    let budget = crate::config::total_ram_mb().map(|mb| (mb / 2) as usize);
    let shortlist = curated_within_budget(budget);
    let mut items: Vec<String> = shortlist
        .iter()
        .map(|m| {
            let tag = CURATED
                .iter()
                .find(|(cm, _)| cm == m)
                .map(|(_, t)| *t)
                .unwrap_or("");
            let mut s = format!(
                "{model} — {dim}-dim, ~{mb} MB [{tag}]",
                model = m.as_str(),
                dim = m.dimension(),
                mb = m.size_mb(),
                tag = tag
            );
            if cached(*m) {
                s.push_str(" (downloaded)");
            }
            if m.as_str() == default_model {
                s.push_str("  <- current");
            }
            s
        })
        .collect();
    items.push(format!("Show all {} models…", EmbeddingModel::ALL.len()));
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let default_idx = shortlist
        .iter()
        .position(|m| m.as_str() == default_model)
        .unwrap_or(0);
    let pick = Select::with_theme(theme)
        .with_prompt("Local embedding model")
        .default(default_idx)
        .items(&refs)
        .interact()
        .map_err(canceled)?;
    let model = if pick < shortlist.len() {
        shortlist[pick]
    } else {
        prompt_all_models(theme, &default_model)?
    };

    let search = SearchConfig {
        provider: "local".into(),
        model: model.as_str().to_string(),
        ..SearchConfig::default()
    };
    if cached(model) {
        println!("  ✓ {} is already downloaded", model.as_str());
        return Ok(search);
    }
    println!(
        "  {} is ~{} MB. Without downloading now it is fetched on first index build.",
        model.as_str(),
        model.size_mb()
    );
    if Confirm::with_theme(theme)
        .with_prompt("Download it now?")
        .default(true)
        .interact()
        .map_err(canceled)?
    {
        download_and_check(model)?;
    }
    Ok(search)
}

/// The full 44-model catalog, for users who want something the curated list
/// leaves out.
fn prompt_all_models(theme: &ColorfulTheme, default_model: &str) -> Result<EmbeddingModel> {
    let labels: Vec<String> = EmbeddingModel::ALL
        .iter()
        .map(|m| {
            format!(
                "{} — {}-dim, ~{} MB: {}",
                m.as_str(),
                m.dimension(),
                m.size_mb(),
                m.description()
            )
        })
        .collect();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let default_idx = EmbeddingModel::ALL
        .iter()
        .position(|m| m.as_str() == default_model)
        .unwrap_or(0);
    let pick = Select::with_theme(theme)
        .with_prompt("All embedding models")
        .default(default_idx)
        .items(&refs)
        .interact()
        .map_err(canceled)?;
    Ok(EmbeddingModel::ALL[pick])
}

fn prompt_openai_embed(
    theme: &ColorfulTheme,
    existing: Option<&SearchConfig>,
) -> Result<SearchConfig> {
    let key_default = existing
        .filter(|s| s.provider == "openai")
        .map(|s| s.openai_api_key_env.clone())
        .unwrap_or_else(|| "OPENAI_API_KEY".to_string());
    let openai_api_key_env = Input::with_theme(theme)
        .with_prompt("Env var holding the OpenAI key")
        .default(key_default)
        .interact()
        .map_err(canceled)?;
    check_env(&openai_api_key_env);
    Ok(SearchConfig {
        provider: "openai".into(),
        ..SearchConfig::default()
    })
}

/// Build the provider once, which fetches the HF weights into
/// ~/.research/models, then embed one sentence to prove it works.
fn download_and_check(model: EmbeddingModel) -> Result<()> {
    let cache = crate::config::research_dir().join("models");
    println!("  Downloading {} ...", model.as_str());
    let provider = FastembedProvider::new(model, Some(cache.clone()))
        .context("model download failed (it will be retried on first index build)")?;
    let result = provider
        .embed_document("research-agent onboarding check")
        .context("embed smoke test failed")?;
    println!("  ✓ downloaded and verified ({}-dim vectors)", result.dim());
    Ok(())
}

/// Expand a leading `~` so pasted home-relative paths work.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_list_is_sane() {
        let models: Vec<EmbeddingModel> = CURATED.iter().map(|(m, _)| *m).collect();
        assert!(models.contains(&EmbeddingModel::BGESmallENV15));
        // No image models, no duplicates, and every entry round-trips through
        // the config string (it must parse back at runtime).
        assert!(models.iter().all(|m| !m.is_image_model()));
        let mut unique = models.clone();
        unique.sort_by_key(|m| m.as_str());
        unique.dedup();
        assert_eq!(unique.len(), models.len());
        assert!(models
            .iter()
            .all(|m| EmbeddingModel::parse(m.as_str()) == Ok(*m)));
    }

    #[test]
    fn low_budget_keeps_tiny_drops_large() {
        let fits = curated_within_budget(Some(1024));
        assert!(fits.contains(&EmbeddingModel::BGESmallENV15Q));
        assert!(!fits.contains(&EmbeddingModel::BGELargeENV15));
    }

    #[test]
    fn tiny_budget_falls_back_to_quantized_small() {
        let fits = curated_within_budget(Some(256));
        assert_eq!(fits, vec![EmbeddingModel::BGESmallENV15Q]);
    }

    #[test]
    fn big_or_unknown_budget_shows_everything() {
        assert_eq!(curated_within_budget(Some(8192)).len(), CURATED.len());
        assert_eq!(curated_within_budget(None).len(), CURATED.len());
    }
}
