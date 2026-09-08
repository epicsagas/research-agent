# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-08

### Added
- **Citation intents**: `research references <id> --intents` and the MCP `paper_references` tool with `intents: true` label existing citation-graph edges with Semantic Scholar's per-edge intents (`background`, `methodology`, `result`) plus its `isInfluential` flag, stored in the `citations.context` column and shown next to each edge. Labeling only touches edges the graph already holds and never invents new ones. Semantic Scholar classifies only a fraction of edges upstream (roughly 30-80% in sampling), so partial labeling is the expected result, and the command reports how many edges were left unlabeled.
- **Citation graph**: `research references <paper-id>` and the MCP `paper_references` tool fetch a paper's references from OpenAlex (`referenced_works`), ingest newly seen referenced papers into the library (deduped by OpenAlex id then DOI), and persist paper-to-paper edges in a new `citations` table (created automatically on next open). Powers "what should I read next from this paper"; re-runs are idempotent. The reverse direction ships too: `--cited-by` (CLI) or `direction: "cited_by"` (MCP) lists the works citing the paper via the OpenAlex `cites:` filter, first page of 200.
- **Europe PMC and preprint sources**: `research ingest --source europepmc` (PubMed/MEDLINE via the free, keyless Europe PMC API) and `--source preprints` (bioRxiv, medRxiv, and other preprint servers via the `SRC:PPR` filter); both are also MCP `ingest` sources and included in `all`. Results carry title, authors, abstract, year, journal/venue, DOI, and a europepmc.org article URL.
- **OpenAlex source**: `research ingest --source openalex` (and `ingest` MCP source `openalex`) searches 250M+ works via the free, keyless OpenAlex API; abstracts are reconstructed from the inverted index, and papers carry a new `openalex_id` field (automatic schema migration to version 2).
- **BibTeX/CSL-JSON import**: `research import <file|dir>` and the MCP `import_papers` tool parse `.bib`/`.bibtex`/`.json` files — e.g. a Zotero export. Papers whose DOI is already in the library are skipped; per-file failures are reported without blocking the batch.
- **Hybrid search**: `research query` and `query_papers` now fuse FTS5 lexical hits with semantic hits from a local vector index (llm-kernel `TurbovecIndex`, RRF fusion), persisted at `~/.research/embeddings.idx` and rebuilt automatically when stale (`research index --rebuild` forces it). The default embedding backend is a bundled small ONNX model (BGESmallENV15, 384-dim) using CoreML on Apple Silicon, DirectML on Windows, CPU elsewhere; `[search]` in `config.toml` switches to OpenAI (BYOK) or a different local model. Every failure degrades gracefully to lexical-only search.
- **Full-body storage for PDFs**: PDF ingest no longer discards the extracted text — the body is section-marked (`## Heading`) and stored in a separate `paper_bodies` table, searched by FTS5 and embedded for hybrid search. Read it back with `research read <id> --body` or the MCP `paper_body` tool (tool responses are capped at 40k chars).
- MCP tools `import_papers` and `paper_body` (13 → 15 tools).
- Paper indexing with arXiv and Semantic Scholar source adapters
- SQLite storage with FTS5 full-text search
- Research topic management with hierarchical trees
- Knowledge gap analysis
- Research report generation
- Reading status tracking
- CLI with init, ingest, index, query, gaps, report, topics, status, read commands
- **Release pipeline (cargo-dist 0.31.0)**: `dist-workspace.toml` (SSOT) + generated `.github/workflows/release.yml`. Builds 7 targets (Linux gnu/musl, macOS Intel/ARM, Windows) with shell + PowerShell + Homebrew installers. Git tag `vX.Y.Z` triggers the release; the hand-written `install.sh`/`install.ps1` are injected into release artifacts so the plugin's auto-installer works.
- README now follows the epicsagas install standard (curl → irm → Homebrew) with a matching Updating table.
- **MCP stdio server** (`research mcp`): exposes research-agent as 13 MCP tools (init, ingest, index_rebuild, query_papers, topic_brief, gaps_record, list_gaps, report_material, report_save, topics_list, topic_add, state, update_read) so an AI agent can drive the research flow directly. Adaptive dispatch: MCP is the primary interface for MCP hosts, CLI is the fallback for terminals/CI.
- `mcp` cargo feature (on by default): gates `rmcp` + `schemars`. Build CLI-only with `--no-default-features`.
- Plugin layer: `mcp_config.json`, `.claude-plugin/hooks.json` (SessionStart auto-install via `registry/scripts/install.js`), and cross-platform `install.sh` / `install.ps1`.
- Real arXiv API integration: `ArxivSource` now calls the Atom XML endpoint (`export.arxiv.org/api/query`) and parses results via `quick-xml`
- Real Semantic Scholar API integration: `SemanticScholarSource` now calls the Graph API (`api.semanticscholar.org/graph/v1/paper/search`) and deserializes JSON via `serde`
- Local PDF ingest: new `PdfSource` adapter extracts text from PDF files using `pdf-extract`
- CLI: `research ingest --source pdf --path <file|dir>` ingests single PDFs or all PDFs in a directory
- LLM config via `~/.research/config.toml` `[llm]` section: `gaps` and `report` commands use a real LLM when `provider`, `model`, and `api_key_env` are configured (key read from that env var at call time)
- `research topics add <name> --parent <id>`: creates sub-topics with automatic depth calculation
- `research ingest <query> --topic <id>`: automatically links ingested papers to the given topic
- `research read <id> --rating <1-5>`: records a 1–5 star rating on a paper
- Paper detail view (`research read <id>`) now shows `Rating: N/5` when set
- `Paper.rating` column added via automatic SQLite migration (safe for existing databases)

### Changed
- Release targets drop the two `*-musl` platforms: the bundled ONNX runtime has no musl prebuilts and requires glibc ≥ 2.38 (Linux binaries now target Ubuntu 24.04+).
- Upgraded `llm-kernel` from `0.9.x` to `0.10` (0.10.0 released). This project consumes only the default provider catalog, which is unchanged in 0.10.0 — no behavior regression. Closes #10.
- README restructured to the epicsagas install standard (curl → irm → Homebrew) with a matching Updating table and an MCP-server section.
- `SqliteStore::open` now sets a 5s `busy_timeout`, so concurrent handles (e.g. parallel MCP tool calls each running `init_schema`) no longer surface "database is locked".
- `research ingest` query argument is now optional (not required when `--source pdf`)
- `research topics` now uses subcommands: `topics list` and `topics add <name>`
- `research topics list` now indents sub-topics by depth (2 spaces per level)

### Fixed
- **Topic-scoped analysis**: `analyze_gaps` and `generate_report` now build their LLM context from papers belonging to the requested topic only. Previously both called `list_papers(N)` returning arbitrary recent papers, so gap analysis and reports described the wrong papers (and `generate_report` emitted the same 5 arbitrary papers for every topic). Adds `IndexStore::list_papers_by_topic`; empty topics return an explicit placeholder instead of leaking unlinked papers.
- Pre-existing `cargo fmt` violations in `src/config.rs` and `src/mcp/server.rs` resolved.
