<div align="center">

# research-agent

> Your long-term research memory — papers indexed, gaps found, reports generated

[![CI](https://github.com/epicsagas/research-agent/actions/workflows/ci.yml/badge.svg)](https://github.com/epicsagas/research-agent/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

## What is this?

research-agent is a personal research assistant that indexes papers and articles, identifies knowledge gaps, and generates research reports. It tracks what you've read, what's missing, and what to read next.

## Features

| | Feature | Why it matters |
|--|---------|----------------|
| 📚 | Paper indexing | arXiv, Semantic Scholar, and local PDF support |
| 🔍 | Full-text search | FTS5 finds any paper or note instantly |
| 🎯 | Gap analysis | Identifies what you haven't read yet — and why it matters |
| 📊 | Research reports | Auto-generated literature reviews and state-of-field summaries |
| 📂 | Topic trees | Organize research hierarchically with sub-topics |
| 📖 | Reading tracker | Queue, track, and rate what you've read |
| 🤖 | MCP server | `research serve` lets an LLM agent drive the whole flow via MCP tools |
| ⚡ | Single binary | No runtime, no server — just `research` |

## Quick Start

Install it as a plugin in your AI agent — no Rust toolchain needed:

| Agent | Install |
|-------|---------|
| Claude Code | `claude plugin marketplace add epicsagas/research-agent` then `claude plugin install research-agent@research-agent` |
| Codex | `codex plugin marketplace add epicsagas/research-agent` then `codex plugin add research-agent` |
| Antigravity (agy) | copy this repository into `~/.gemini/config/plugins/research-agent` |
| Grok Build | `grok plugin install epicsagas/research-agent --trust` |

The plugin auto-installs the `research` binary on session start and exposes
11 MCP tools, so the agent can ingest, search, analyze, and report on its own.

Once installed, ask your agent things like:

- "Ingest recent papers on graph neural network training from arXiv"
- "What gaps are left in my <topic> coverage?"
- "Generate a survey report for <topic>"

## Installation (CLI)

Prefer driving it by hand? The same binary works as a standalone CLI.

```bash
# macOS / Linux — pre-built binary, no Rust required
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — pre-built binary, no Rust required
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent

# cargo-binstall — pre-built binary via Rust toolchain
cargo binstall research-agent

# cargo install — build from source (requires Rust toolchain)
cargo install research-agent
```

## Updating

| Method | Command |
|--------|---------|
| curl installer (macOS/Linux) | Re-run the install script above |
| PowerShell installer (Windows) | Re-run the install command above |
| Homebrew | `brew upgrade research-agent` |
| cargo binstall | `cargo binstall research-agent@latest` |
| cargo install | `cargo install research-agent@latest` |

Verify the installed version:

```bash
research --version
```

## MCP server (agent-driven, primary interface)

`research serve` starts a **stdio MCP server** so an LLM agent (Claude Code,
Codex, …) can drive research-agent directly via MCP tools — control is
inverted: instead of a human typing CLI commands, the agent discovers and calls
the tools. The CLI remains a secondary interface for terminals/CI.

```bash
research serve
```

Adaptive dispatch — **MCP first, CLI fallback**:

| Environment | Detection | Interface |
|---|---|---|
| MCP host (Claude Code / Codex) | plugin `mcp_config.json` loads | MCP tools (1st-class) |
| Terminal / CI script | `research <cmd>` directly | CLI (fallback) |
| Binary missing | plugin SessionStart hook | auto-install from GitHub Release |

**Tools** (11): `init` · `ingest` · `index_rebuild` · `query_papers` ·
`analyze_gaps` · `list_gaps` · `generate_report` · `topics_list` · `topic_add` ·
`state` · `update_read`.

Smoke test the server over raw JSON-RPC:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research serve 2>/dev/null
```

Load it as a plugin so the host auto-discovers the tools and auto-installs the
`research` binary on session start (SessionStart hook). The `mcp` cargo feature
is **on by default**; build CLI-only with `cargo build --no-default-features`.

## Commands

| Command | Description |
|---------|-------------|
| `research init` | Initialize research workspace |
| `research ingest <query> [--source arxiv\|s2\|all]` | Ingest papers from arXiv or Semantic Scholar |
| `research ingest --source pdf --path <file\|dir>` | Ingest local PDF files |
| `research index [--rebuild]` | Build or rebuild search index |
| `research query <q>` | Search papers and notes |
| `research gaps [--topic <id>]` | Analyze knowledge gaps |
| `research report --topic <id>` | Generate research report |
| `research topics list` | List all topics |
| `research topics add <name>` | Add a new topic |
| `research read <id> [--status <status>] [--rating <1-5>]` | Update reading status or rating |
| `research status` | Show research state overview |
| `research serve` | Start the stdio MCP server (agent-driven mode) |

Every subcommand also accepts a global `--db <path>` flag to use a specific
database instead of `~/.research/research.db` — useful for isolated or test
workspaces.

## Requirements

- Rust 1.92+ (only if building from source; pre-built binaries need nothing)
- Data lives at `~/.research/` (`research.db` index, `config.toml` settings)
- An LLM for `gaps` / `report` — set `[llm]` in `~/.research/config.toml` (`provider`, `model`, `api_key_env`); the API key is read from that env var at call time

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

## License

[APACHE-2](LICENSE).
