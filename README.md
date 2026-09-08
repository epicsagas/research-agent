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
| 🤖 | MCP server | 16 tools your agent drives directly over stdio — no API key needed |
| 🧠 | Agent-native analysis | Gap analysis and reports run inside your agent: tools hand over structured state, the agent reasons, results are persisted |
| 🔗 | Citation graph | Paper-to-paper reference edges from OpenAlex, forward and reverse, power "what should I read next from this paper" |
| 📚 | Paper indexing | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), bioRxiv-style preprints, and local PDF support |
| 🔍 | Hybrid search | FTS5 lexical + local-ONNX semantic hits, RRF-fused — works offline |
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
codex plugin add research-agent@research-agent
```

### Antigravity (agy)

```bash
agy plugin install https://github.com/epicsagas/research-agent
agy plugin enable research-agent
```

### Grok Build

```bash
grok plugin install epicsagas/research-agent --trust
```

### Hermes Agent

```bash
hermes plugins install https://github.com/epicsagas/research-agent
hermes plugins enable research-agent
```

Hermes loads the root `plugin.yaml` and `register(ctx)` in `__init__.py`.
It has no MCP support, so the agent drives `research` through the bundled
skill's CLI commands — install the binary first (`brew install
epicsagas/tap/research-agent` or the curl installer); the SessionStart
auto-install hook does not apply to hermes. If skills_guard blocks the
install scan, set `plugins.scan_on_install: false` in the hermes config.

The plugin auto-installs the `research` binary on session start and exposes
15 MCP tools, so the agent can ingest, search, analyze with its own model, and file reports on its own.

Once installed, ask your agent things like:

- "Ingest recent papers on graph neural network training from arXiv"
- "What gaps are left in my <topic> coverage?"
- "Generate a survey report for <topic>"

## How your agent uses it

The plugin auto-installs the `research` binary on session start and starts the
**stdio MCP server** (`research mcp`) — the agent discovers and calls the tools
directly; no human typing CLI commands.

**Tools** (16): `init` · `ingest` · `index_rebuild` · `import_papers` ·
`paper_body` · `paper_references` · `query_papers` · `topic_brief` ·
`gaps_record` · `list_gaps` · `report_material` · `report_save` ·
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
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all]` | Ingest papers from arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), or preprint servers (bioRxiv, medRxiv, …) |
| `research ingest --source pdf --path <file\|dir>` | Ingest local PDF files (full body text is stored and searchable) |
| `research import <file\|dir>` | Import BibTeX/BibLaTeX or CSL-JSON files (e.g. a Zotero export) |
| `research index [--rebuild]` | Build or rebuild search index (FTS + vector index) |
| `research query <q>` | Search papers — hybrid lexical+semantic when embeddings are available |
| `research gaps [--topic <id>]` | Analyze knowledge gaps (CLI: uses `[llm]` if configured) |
| `research report --topic <id>` | Generate research report (CLI: uses `[llm]` if configured) |
| `research topics list` | List all topics |
| `research topics add <name>` | Add a new topic |
| `research read <id> [--status <status>] [--rating <1-5>]` | Update reading status or rating |
| `research read <id> --body` | Print a paper's stored body text |
| `research status` | Show research state overview |
| `research mcp` | Start the stdio MCP server (alias: `serve`) |

Every subcommand also accepts a global `--db <path>` flag to use a specific
database instead of `~/.research/research.db` — useful for isolated or test
workspaces.

## Requirements

- Rust 1.92+ (only if building from source; pre-built binaries need nothing)
- Data lives at `~/.research/` (`research.db` index, `config.toml`, `embeddings.idx`)
- Linux binaries target glibc ≥ 2.38 (Ubuntu 24.04+); musl is not built because
  the bundled ONNX runtime has no musl prebuilts
- Optional: an LLM for `research gaps` / `research report` **from the CLI only** — set `[llm]` in `~/.research/config.toml` (`provider`, `model`, `api_key_env`); the key is read from that env var at call time. The MCP flow never needs it
- Optional: hybrid search tuning via `[search]` in `~/.research/config.toml` —
  `provider = "local"` (default, bundled ONNX model) or `"openai"` with
  `openai_api_key_env`; `model` picks the local model (default `BGESmallENV15`,
  a small 384-dim model that runs on CPU). See [ROADMAP.md](ROADMAP.md) for
  what is deliberately not built yet

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the [ROADMAP](ROADMAP.md). PRs welcome.

## License

[APACHE-2](LICENSE).
