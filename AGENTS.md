# research-agent — Agent Guide

research-agent — personal long-term research assistant: CLI + stdio MCP server
that indexes papers (arXiv, Semantic Scholar, local PDFs) into SQLite with FTS5,
tracks a topic hierarchy, runs LLM knowledge-gap analysis and report generation.
Also ships as a plugin for Claude Code, Codex, Antigravity, Grok Build, and
Hermes, exposing 18 MCP tools. Gap analysis and reports are agent-native:
tools hand over structured topic state and persist what the host model
concludes; the
optional `[llm]` config only powers the standalone CLI.

## Commands

- Build: `cargo build` | Release: `cargo build --release`
- Test: `cargo test` | Single: `cargo test <name>` | Output: `cargo test -- --nocapture`
- Lint: `cargo clippy --all-targets -- -D warnings` | Format: `cargo fmt --all --check` / `cargo fmt`
- Audit: `cargo audit`
- Run: `cargo run -- --db /tmp/r.db init`, then `ingest <query>`, `status`;
- MCP: `cargo run -- mcp` (stdio JSON-RPC; 18 tools)
- Release: `dist generate` after editing `dist-workspace.toml`; releases fire on
  a pushed `vX.Y.Z` tag. Homebrew (`publish-homebrew-formula`) and crates.io
  (`custom-publish-crates` → `.github/workflows/publish-crates.yml`) are both
  cargo-dist publish jobs on that same Release run (`publish-jobs =
  ["homebrew", "./publish-crates"]`, alcove pattern). Do not add a second
  Homebrew pusher. `dist generate` regenerates release.yml and drops the
  hand-added `continue-on-error: true` on `publish-homebrew-formula` and the
  `install.sh`/`install.ps1` copy into artifacts — re-add both after
  regenerating. The repo's default workflow permission must stay `write` or
  the release cannot create the GitHub Release (HTTP 403)

## Project Structure

- `src/main.rs` — clap CLI surface (global `--db`)
- `src/adapters/` — hexagonal implementations: `arxiv_source`, `semantic_scholar_source`,
  `openalex_source`, `pdf_source`, `bib_importer`, `sqlite_store`, LLM/report adapters
- `src/ports/` — trait boundaries (`paper_source::PaperSource`, `index_store::IndexStore`, …);
  prefer adding a port + fake over a network-dependent test
- `src/application/` — pipeline orchestration (ingest, import, keyword enrichment,
  gap analysis, reports)
- `src/domain/` — `Paper`, statuses, ratings
- `plugin.json`, `.claude-plugin/`, `.codex-plugin/` — plugin manifests, MCP tool
  wiring, SessionStart auto-install hook

## Rules

- Rust 2024 edition, MSRV 1.92; errors via `ResearchError` / `crate::error::Result`
- **No network in unit tests.** External APIs belong behind `PaperSource`; live
  round-trips are `#[ignore]`-gated (arXiv rate-limits hard — HTTP 429).
- HTTP responses are checked before parsing: a non-2xx from arXiv/S2 is a typed
  `ResearchError::Source`, never an empty result.
- Version lockstep: `Cargo.toml` version, `plugin.json`, `plugin.yaml`,
  `.claude-plugin/plugin.json`, `.codex-plugin/plugin.json` and
  `.grok-plugin/plugin.json` must move together.
- Tests are offline by default; keep `cargo test` green with no network.
