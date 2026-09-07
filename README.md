<div align="center">

# research-agent

> Your long-term research memory — papers indexed, gaps found, reports generated

<p align="center">
  <a href="https://github.com/epicsagas/research-agent/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=ffd700&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/issues"><img alt="Issues" src="https://img.shields.io/github/issues/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=ff6b6b&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/commits/main"><img alt="Last commit" src="https://img.shields.io/github/last-commit/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=58a6ff&logo=git&logoColor=white" /></a>
</p>
<p align="center">
  <a href="https://github.com/epicsagas/research-agent/releases"><img alt="Version" src="https://img.shields.io/github/v/release/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=fc8d62&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/epicsagas/research-agent/total?style=for-the-badge&labelColor=0d1117&color=3498db&logo=github&logoColor=white" /></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-3fb950?style=for-the-badge&labelColor=0d1117" /></a>
</p>

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
| 🤖 | MCP server | `research mcp` lets an AI agent drive the whole flow via MCP tools — no API key needed |
| ⚡ | Single binary | No runtime, no server — just `research` |

## Quick Start

Install it as a plugin in your AI agent — no Rust toolchain needed:

### Claude Code

```bash
claude plugin marketplace add epicsagas/research-agent
claude plugin install research-agent@research-agent
```

### Codex

```bash
codex plugin marketplace add epicsagas/research-agent
codex plugin add research-agent
```

### Antigravity (agy)

```bash
git clone https://github.com/epicsagas/research-agent ~/.gemini/config/plugins/research-agent
```

### Grok Build

```bash
grok plugin install epicsagas/research-agent --trust
```

The plugin auto-installs the `research` binary on session start and exposes
13 MCP tools, so the agent can ingest, search, analyze with its own model, and file reports on its own.

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
```

## Updating

| Method | Command |
|--------|---------|
| curl installer (macOS/Linux) | Re-run the install script above |
| PowerShell installer (Windows) | Re-run the install command above |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Verify the installed version:

```bash
research --version
```

## MCP server (agent-driven, primary interface)

`research mcp` starts a **stdio MCP server** so an AI agent (Claude Code,
Codex, …) can drive research-agent directly via MCP tools — control is
inverted: instead of a human typing CLI commands, the agent discovers and calls
the tools. The CLI remains a secondary interface for terminals/CI.

```bash
research mcp
```

Adaptive dispatch — **MCP first, CLI fallback**:

| Environment | Detection | Interface |
|---|---|---|
| MCP host (Claude Code / Codex) | plugin `mcp_config.json` loads | MCP tools (1st-class) |
| Terminal / CI script | `research <cmd>` directly | CLI (fallback) |
| Binary missing | plugin SessionStart hook | auto-install from GitHub Release |

**Tools** (13): `init` · `ingest` · `index_rebuild` · `query_papers` ·
`topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` ·
`topics_list` · `topic_add` · `state` · `update_read`.

Gap analysis and reports run **inside the agent**: `topic_brief` and
`report_material` hand over the structured library state, the agent reasons
over it with its own model, and `gaps_record` / `report_save` persist the
findings. No `[llm]` config or API key is required; the `[llm]` section stays
optional for gap analysis and reports from the standalone CLI.

Smoke test the server over raw JSON-RPC:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
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
| `research mcp` | Start the stdio MCP server (alias: `serve`) |

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
