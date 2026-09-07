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

research-agent is a **research server for AI agents**: it indexes papers (arXiv,
Semantic Scholar, local PDFs) into a local SQLite library, and your agent drives
it over MCP — ingesting, searching, tracking reading progress, analyzing
coverage gaps with its own model, and filing survey reports. No API key
required.

The same binary also works as a standalone CLI for terminals and scripts.

## Features

| | Feature | Why it matters |
|--|---------|----------------|
| 🤖 | MCP server | 13 tools your agent drives directly over stdio — no API key needed |
| 🧠 | Agent-native analysis | Gap analysis and reports run inside your agent: tools hand over structured state, the agent reasons, results are persisted |
| 📚 | Paper indexing | arXiv, Semantic Scholar, and local PDF support |
| 🔍 | Full-text search | FTS5 finds any paper or note instantly |
| 📂 | Topic trees | Organize research hierarchically with sub-topics |
| 📖 | Reading tracker | Queue, track, and rate what you've read |
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

## How your agent uses it

The plugin auto-installs the `research` binary on session start and starts the
**stdio MCP server** (`research mcp`) — the agent discovers and calls the tools
directly; no human typing CLI commands.

**Tools** (13): `init` · `ingest` · `index_rebuild` · `query_papers` ·
`topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` ·
`topics_list` · `topic_add` · `state` · `update_read`.

Analysis is **agent-native**: `topic_brief` and `report_material` hand over the
structured library state (papers, reading progress, recorded gaps, coverage),
the agent reasons over it with its own model, and `gaps_record` / `report_save`
persist the findings. No `[llm]` config or API key is required anywhere in the
MCP flow.

Smoke test the server over raw JSON-RPC:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

The `mcp` cargo feature is **on by default**; build CLI-only with
`cargo build --no-default-features`.

## Standalone CLI (secondary)

Prefer driving it by hand? The same binary works as a plain CLI.

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

## Commands

| Command | Description |
|---------|-------------|
| `research init` | Initialize research workspace |
| `research ingest <query> [--source arxiv\|s2\|all]` | Ingest papers from arXiv or Semantic Scholar |
| `research ingest --source pdf --path <file\|dir>` | Ingest local PDF files |
| `research index [--rebuild]` | Build or rebuild search index |
| `research query <q>` | Search papers and notes |
| `research gaps [--topic <id>]` | Analyze knowledge gaps (CLI: uses `[llm]` if configured) |
| `research report --topic <id>` | Generate research report (CLI: uses `[llm]` if configured) |
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
- Optional: an LLM for `research gaps` / `research report` **from the CLI only** — set `[llm]` in `~/.research/config.toml` (`provider`, `model`, `api_key_env`); the key is read from that env var at call time. The MCP flow never needs it

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

## License

[APACHE-2](LICENSE).
