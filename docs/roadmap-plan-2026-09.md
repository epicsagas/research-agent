# Implementation plan: remaining roadmap items (2026-09-08)

Derived from [roadmap-research-2026-09.md](roadmap-research-2026-09.md). Two work packages, both scoped down from what the roadmap originally implied, because the research changed what the right work is.

## Scope

| Package | Delivers | Explicitly out of scope |
|---|---|---|
| A. PDF page anchors + evidence snippets | Page markers in stored bodies; search and `paper_body` return the matching text with its section and page | New PDF crate; per-page table; forced re-ingest of existing bodies |
| B. Importer field recovery | Abstracts, tags, Zotero-native JSON, DOI normalization | Zotero live sync, the local HTTP API, write support, conflict resolution |

Package B is deliberately *not* the roadmap item as written. The roadmap item is Zotero live integration, gated on the import path proving insufficient. The research found that gate closed and found the importer discarding data its own input files carry. Fixing that is both the honest prerequisite to reopening the gate and the cheaper way to address the pain that motivated it.

---

## Package A: PDF page anchors and evidence snippets

### A1. Per-page extraction with inline markers

`src/adapters/pdf_source.rs`

Switch line 65 from `extract_text_from_mem` to `extract_text_from_mem_by_pages`, then join the pages with an inline marker between them. Markers stay inside the body text so `paper_bodies` keeps one row per paper and the FTS5 shadow table and its three triggers stay untouched.

Marker format: `<!-- page N -->` on its own line. HTML-comment syntax so it is inert if a body is ever rendered as markdown, and distinctive enough to match on and to strip.

Guard the silent-truncation trap the research identified: `extract_text_*_by_pages` treats a failing page as end-of-document. Compare the returned page count against the document page count and warn rather than silently storing a truncated body.

Section-heading marking (`prepare_body`) runs after page joining, unchanged.

### A2. Anchor resolution

New: given a body and a byte offset, resolve the enclosing section and page by scanning backwards for the nearest preceding `## ` heading and `<!-- page N -->` marker. Pure function, no I/O, trivially testable.

Both anchors are returned. Section is the more durable one across renderings; page is the one people ask for.

### A3. Snippet retrieval

`src/adapters/sqlite_store.rs`

Body FTS hits currently return only the paper row, so a match says which paper without saying where. Use SQLite FTS5's built-in `snippet()` on `bodies_fts` to return the matching text, then resolve its anchors via A2.

New port method on `IndexStore` for evidence-bearing body search, returning paper plus snippet plus anchors. Existing `search_papers` behavior is left alone.

### A4. Surfacing

- CLI: `research query <q> --evidence` prints the matching snippet with its section and page under each hit.
- MCP: `paper_body` gains an optional `query` parameter; when present it returns matching snippets with anchors instead of the truncated blob. Backwards compatible — absent `query` keeps today's behavior.

### A5. Re-ingest path

`research reingest [--missing-pages]`, walking papers with a non-null `pdf_path`, re-extracting, and replacing the body via the already-idempotent `set_paper_body`.

Detection of stale bodies: `body NOT LIKE '%<!-- page %'`. Per-paper, resumable, never forced. A moved or deleted PDF is skipped with a warning, not a failure — `pdf_path` is an absolute canonicalized path and will break for some users.

Reminder in the output: re-ingested papers need `research index --rebuild`, since the vector index embeds the body.

---

## Package B: Importer field recovery

`src/adapters/bib_importer.rs`, `src/application/paper_import.rs`

### B1. Abstract

Read `abstract` from BibTeX and `abstract` from CSL-JSON into `paper.abstract_text`. This is the highest-value fix in the package: the field feeds both gap analysis and the search index, so its absence makes imported papers second-class.

### B2. Tags

Read BibTeX `keywords` and CSL-JSON `keyword`, split on commas and semicolons, trim, drop empties, into `paper.tags`.

### B3. Zotero-native JSON

`.json` is currently hardcoded to the CSL parser, so Zotero's own JSON export — a reasonable pick in the export dialog — fails with a parse error. Content-sniff instead: Zotero JSON items carry `itemType` and use `abstractNote`/`creators`, CSL items carry `type` and `container-title`. Dispatch on that, and keep the error message naming both formats when neither matches.

### B4. DOI normalization

Normalize before the dedupe lookup: strip a leading `https://doi.org/` or `http://dx.doi.org/`, trim, lowercase. DOIs are case-insensitive by specification, and Zotero's DOI field content varies by translator.

Applies to both the stored value and the lookup key, so normalization is consistent for papers already in the library and papers arriving now.

Not in scope: a `UNIQUE` constraint on `papers.doi`. That would need a migration and would change ingest behavior, which is a separate decision from fixing the importer.

---

## Verification

Each package carries tests that fail if its logic breaks.

- A1: a fixture PDF extracts with page markers; page count matches the document.
- A2: anchor resolution over a synthetic body — offsets before any marker, between markers, and after the last one.
- A3: a body search returns a snippet containing the query term with correct anchors.
- B1/B2: a BibTeX and a CSL fixture carrying abstract and keywords round-trip into `abstract_text` and `tags`.
- B3: a Zotero-native JSON fixture parses; a CSL fixture still parses; garbage still errors.
- B4: `10.1/X`, `https://doi.org/10.1/X`, and `10.1/x` all dedupe to one paper.

Full suite, clippy, and fmt gate the merge. Live-network tests stay `#[ignore]`, consistent with existing practice in this repo.

## Sequencing

B before A. Package B is smaller, has no interactions with the storage layer, and its tests are pure parsing. Package A touches extraction, storage, search, CLI, and MCP, so it lands second on a clean base.

## What this plan does not do

It does not implement Zotero live integration. The gate stays closed until the importer fix has shipped and real usage shows whether the remaining pain justifies a sync engine. That judgment needs evidence that does not exist yet — the local library shows zero imports and zero PDF ingests to date.

It does not add a PDF dependency. The premise that one was needed proved false, and a native dependency across five distribution targets is a cost this project has already paid once, when the bundled ONNX runtime forced dropping both musl targets.
