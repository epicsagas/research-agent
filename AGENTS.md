# AGENTS.md

research-agent — personal long-term research assistant: CLI + stdio MCP server
that indexes papers (arXiv, Semantic Scholar, local PDFs) into SQLite with FTS5,
tracks a topic hierarchy, runs LLM knowledge-gap analysis and report generation.
Also ships as a Claude/Codex plugin exposing 11 MCP tools.

## Commands

- Build: `cargo build` | Release: `cargo build --release`
- Test: `cargo test` | Single: `cargo test <name>` | Output: `cargo test -- --nocapture`
- Lint: `cargo clippy --all-targets -- -D warnings` | Format: `cargo fmt --all --check` / `cargo fmt`
- Audit: `cargo audit`
- Run: `cargo run -- --db /tmp/r.db init`, then `ingest <query>`, `status`;
- MCP: `cargo run -- serve` (stdio JSON-RPC; 11 tools)
- Release: `dist generate` after editing `dist-workspace.toml`; releases fire on
  a pushed `vX.Y.Z` tag

## Project Structure

- `src/main.rs` — clap CLI surface (global `--db`)
- `src/adapters/` — hexagonal implementations: `arxiv_source`, `semantic_scholar_source`,
  `sqlite_store`, LLM/report adapters
- `src/ports/` — trait boundaries (`paper_source::PaperSource`, `index_store::IndexStore`, …);
  prefer adding a port + fake over a network-dependent test
- `src/application/` — pipeline orchestration (ingest, gap analysis, reports)
- `src/domain/` — `Paper`, statuses, ratings
- `plugin.json`, `.claude-plugin/`, `.codex-plugin/` — plugin manifests, MCP tool
  wiring, SessionStart auto-install hook

## Rules

- Rust 2024 edition, MSRV 1.92; errors via `ResearchError` / `crate::error::Result`
- **No network in unit tests.** External APIs belong behind `PaperSource`; live
  round-trips are `#[ignore]`-gated (arXiv rate-limits hard — HTTP 429).
- HTTP responses are checked before parsing: a non-2xx from arXiv/S2 is a typed
  `ResearchError::Source`, never an empty result.
- Version lockstep: `Cargo.toml` version, `plugin.json`, `.claude-plugin/plugin.json`
  and `.codex-plugin/plugin.json` must move together.
- Tests are offline by default; keep `cargo test` green with no network.
