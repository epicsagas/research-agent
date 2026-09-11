# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-11

### Fixed
- The one-line installers (install.sh / install.ps1) looked for
  `research-<target>` archives but cargo-dist uploads `research-agent-<target>`
  ones, so every download 404'd. Found by running the released installer
  end-to-end.

### Changed
- Release pipeline: the tag-scoped `release-extras` workflow publishes to
  crates.io (`CARGO_REGISTRY_TOKEN`) and the Homebrew tap (`HOMEBREW_TAP_TOKEN`),
  each a green no-op until its secret is configured; a missing tap token no
  longer marks the whole release run failed; CI cancels superseded runs.

### Added
- **Dashboard UI overhaul**: light theme alongside the dark reading room, a
  mobile layout, full-height modals, and rich text rendering. The UI is
  localized to all ten README languages, with a language picker in the sidebar.

### Fixed
- The dashboard language dropdown no longer overflows the viewport in narrow
  sidebars.
- Session-start auto-install hook: the installer download now times out
  (10 s connect / 60 s transfer) instead of hanging the session on a stalled
  network, runs from a private temp directory, and tolerates redirect
  responses without a `Location` header. The Claude hook command guards on
  `node` being present. Grok installs find the plugin version even without a
  `.claude-plugin/` directory.
- install.sh / install.ps1: network timeouts on every download, PowerShell
  temp dir is cleaned up on failure too, TLS 1.2 pinned for old Windows
  PowerShell 5.1 builds.
- Dashboard hardening: CDN scripts (marked, DOMPurify, KaTeX) are pinned and
  SRI-verified, all responses carry a Content-Security-Policy, and the
  dashboard token is compared in constant time.
- PDF filenames are built from a sanitized arXiv id so a hostile id from
  third-party metadata cannot write outside the PDF directory.
- arXiv search queries are percent-encoded (matching the other sources), so
  `&`, `#`, or `%` in a query can no longer alter the request.

### Removed
- **Embedding search subsystem** (local ONNX via fastembed, and remote OpenAI embeddings): `hybrid_search`, the Turbovec vector index, the `[search]` config section, and the embedding-model step of `research init` are all gone. The vector channel built a full ONNX session per command — the model reloaded on every `research query`, and MCP stdio multiplied that by process count, which is how a 53-paper corpus reached 22 GB RSS. Measured on a 117-paper library, `research query` went from minutes to 0.75 s and peak RSS from gigabytes to 13 MB. FTS5 with generated keywords (below) covers the paraphrase recall the vector channel existed for. Drops `fastembed`, `ort`, `turbovec`, `tokenizers`, `hf-hub`, `onig`, `safetensors` and `ndarray` from the dependency tree. Existing configs keep their `[search]` section — it is ignored, not an error. Leftover `~/.research/embeddings.idx`, `embeddings.state.json`, `embeddings.meta.json` and `~/.research/models/` are not deleted automatically; removing them by hand reclaims a few hundred MB.

### Added
- **`research enrich` — search keywords for paraphrased queries**: FTS5 matches the words a paper contains; keywords cover the ones it does not (synonyms, expanded acronyms, alternative phrasings), stored in a new `keywords` column that the FTS index reads. Three paths, all first-class: with `[llm]` configured the CLI batches its own calls; without one it prints the papers needing keywords so a host agent (Claude Code, any MCP client) can generate them and write them back through `research enrich <ID> --keywords "..."` or the `enrich_paper` MCP tool; a plain CLI user who never runs it keeps working lexical search. Keywords are deliberately separate from `tags`, which are the user's own and get pushed to Zotero. New MCP tools `papers_missing_keywords` and `enrich_paper`; `query_papers` keeps its input schema. Schema version 3 adds the column and recreates `papers_fts` (a virtual table made with `IF NOT EXISTS` keeps its old shape otherwise). A golden-query test asserts that six paraphrase queries miss before enrichment and land in the top 3 after.
- **Web dashboard**: `research dashboard` starts a GET-only HTTP server bound to 127.0.0.1 (default port 7777, `[dashboard] port` in config to change) that serves a single embedded HTML page plus read-only JSON endpoints over the workspace database — library counts, the topic tree, and the paper table with reading status and ratings. A PATCH endpoint updates `reading_status` from the UI. Binding to a non-loopback host requires an access token (generated into `config.toml`), defended against DNS rebinding by a loopback-only `Host` check. GET-only for reads and request routing is a pure `route` function, so it is tested without sockets.
- **Zotero tag push (write slice)**: `research export --to zotero` merges each library paper's tags into its Zotero item over the local API, matched by normalized DOI. Dry-run is the default and prints per-paper actions with zero writes; `--apply` writes, tagging on Zotero's own confirmation dialog for authorization ("Always Allow" for batches — confirmations are capped at five per minute). Items changed in Zotero since the read are skipped and counted, never merged: there is no UI to resolve a conflict. Deliberately CLI-only — no MCP tool, since an agent writing to a user's library is a trust boundary. Reading status and rating stay local: Zotero has no fields for them.
- **Performance benchmarks**: `cargo bench` (criterion) measures lexical query, FTS index rebuild, and body-evidence snippets over a deterministic 500-paper corpus — no network, fully offline. `benches/run.sh` reports the medians as an informational performance dimension for the eval gate; timing never turns the gate red, because absolute thresholds are machine-dependent and shared CI runners swing 10-30%. Same-machine regression gating is deliberately deferred until a reference machine exists (ROADMAP).
- **First-run onboarding**: `research init` in a terminal now walks through the settings that matter — database location, the LLM provider for gap analysis and reports (curated: Anthropic, OpenAI, DeepSeek, OpenRouter, Ollama, LM Studio, plus a custom OpenAI-compatible entry), and the env var holding its key. API keys are never typed into the wizard: it asks for the env var name and reports whether it is set. Ollama and LM Studio are probed (2s timeout) so you pick from models that actually exist. Re-running init offers existing values as defaults instead of resetting. `research init --no-onboard` (or any non-terminal stdin) skips the prompts, so scripts are never blocked. README documents the flow.
- **Self-documenting config.toml**: `Config::save` now writes a template in which every key of the optional `[llm]` section appears — set values as live TOML, unset ones commented out with their defaults — so the file alone shows everything that is configurable. Template output is guaranteed to parse back (`toml::from_str` round-trip tests).

### Fixed
- `research init` no longer wipes an existing `config.toml` (it used to rewrite defaults over any configured `[llm]`). Non-interactive runs leave an existing file byte-for-byte untouched.
- **Zotero read source**: `research ingest --source zotero` reads papers from a running Zotero instance over its local API (`http://localhost:23119/api/`) — the item-to-Paper mapping is shared with the export-file importer, so abstracts, tags, creators, venue, and normalized DOIs all survive the trip. Query-less reads pull the whole library; queries search all fields (`qmode=everything`); pages over Zotero's 100-item page cap. Needs Zotero running with "Allow other applications on this computer to communicate with Zotero" enabled (a 403 error names this); `ZOTERO_BASE_URL` overrides the endpoint for non-default installs. Not part of `--source all`: a personal library is not a discovery source.
- **Page anchors and evidence snippets for PDF bodies**: PDF ingest now extracts per page and writes `<!-- page N -->` markers into the stored body, so a match can be traced back to where it appeared. A page that fails mid-document triggers a warning instead of silently truncating the body (the extraction API treats a failed page as end-of-document). `research query <q> --evidence` prints the matching body text with its section and page, and the MCP `paper_body` tool accepts a `query` parameter that returns located snippets instead of the whole body (far cheaper on an agent's context). Bodies stored before this change keep working and still resolve their section; `research reingest --missing-pages` re-extracts them in place, skipping papers whose source PDF has moved.
- **Importer field recovery**: `research import` now reads abstracts and keywords/tags from BibTeX and CSL-JSON, and accepts Zotero's native JSON export (dispatched by content, since it and CSL-JSON are both `.json`). Abstracts matter most here: the field feeds gap analysis and the search index, so imported papers previously landed semantically empty next to ingested ones.

### Changed
- Papers without a DOI no longer duplicate on re-ingest. The duplicate check is now one shared identity function used by every ingest path (`research ingest`, `research import`, and `--source pdf`), keyed by normalized DOI first, then by the source PDF path, then by normalized title for papers that carry neither. Re-running a whole-library Zotero read or a repeated PDF directory used to insert a fresh row per DOI-less item on every run; the PDF path had no duplicate check at all. Documented trade-off: two genuinely different papers sharing one normalized title, with no DOI and no path, collapse to one row.
- DOIs are normalized (resolver prefix stripped, lowercased) before import dedupe, so `10.1/X`, `https://doi.org/10.1/X`, and `10.1/x` resolve to one paper instead of three rows.

### Fixed
- CSL-JSON import now accepts Zotero's real exports: Zotero writes `issued.date-parts` as strings (`[["2023"]]`, which the CSL-JSON schema allows) and the integer-only parser rejected every such file outright. String parts are parsed as years, and non-numeric parts yield no year instead of failing the whole file. Found by importing an actual Zotero "CSL JSON" export; the fixtures had used integer dates.
- Zotero native-JSON import now understands API-shaped items, whose fields sit under `data` (desktop exports are flat, local/Web-API responses are not). Such files previously reported "0 imported, 0 failed" — accepted by both parsers, producing zero papers from each. The `.json` dispatch also recognizes `itemType` under `data`, without which the file silently landed in the CSL parser.
- Body-evidence anchors no longer mistake a literal bracket in the page text (a citation like `[12]`) for snippet()'s match marker. Picking the citation as the match reported a section or page before the one the match fell under, and because brackets were stripped from the located needle but not the body, the window could fail to locate at all and fall back to the document's first section. Match brackets are now identified by matching the query, and brackets are stripped from both sides before locating.

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
