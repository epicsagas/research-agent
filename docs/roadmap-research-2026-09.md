# Roadmap research: remaining items (2026-09-08)

Investigation of the two items left in [ROADMAP.md](../ROADMAP.md) after citation classification shipped. Both were researched against their actual upstream APIs and the code as it stands, rather than against the assumptions recorded when the items were written. **One of the two roadmap premises turned out to be factually wrong, and the other item's gate turned out to be closed for a reason the roadmap did not anticipate.**

## Summary

| Item | Roadmap premise | Finding | Action |
|---|---|---|---|
| Page-level evidence for PDF bodies | "pdf-extract does not expose page boundaries. Page-anchored citations need a different PDF extraction layer." | **False.** `pdf-extract 0.12` has exposed `extract_text_from_mem_by_pages` since before this project pinned it. No new extraction layer is needed. | Implement. Cost is a handful of lines, not a dependency migration. |
| Zotero live integration | "Gated on the import path proving insufficient in practice." | **Gate closed**, but not because import is sufficient: the importer silently discards fields the export files already contain. A path cannot be judged insufficient while it is throwing away its own input. | Fix the importer. Re-evaluate the gate afterwards on real evidence. |

## Usage evidence

Measured against the local library at `~/.research/research.db`:

- Schema version 1; the current code targets version 2. The database predates the recent feature work.
- 49 papers, every one of them ingested from arXiv.
- 0 papers with a PDF path, 0 papers with a DOI.
- `paper_bodies` and `citations` tables do not exist in that database at all.

So the PDF-body path, the import path, and the citation graph have no real usage history behind them. This matters for gate judgments: an item gated on a path "proving insufficient in practice" cannot have its gate opened by practice that never happened.

---

## Item 1: Page-level evidence for PDF bodies

### Current implementation

Extraction is a single whole-document call at `src/adapters/pdf_source.rs:65`:

```rust
let text = pdf_extract::extract_text_from_mem(&bytes)?;
```

`prepare_body` (`src/adapters/pdf_source.rs:22`) then trims line ends and inserts `## ` markers ahead of about fifteen canonical section headings via one regex (`src/adapters/pdf_source.rs:17`), capping the result at 500,000 chars. The body is stored one row per paper in `paper_bodies` (`src/store/schema.rs:124`) and indexed by the `bodies_fts` trigram FTS5 table (`src/store/schema.rs:133`).

### The premise is false

`pdf-extract` 0.12.0 (MIT, pure Rust, wraps `lopdf`) exposes per-page extraction directly:

| Function | Signature |
|---|---|
| `extract_text_from_mem` | `(&[u8]) -> Result<String, OutputError>` — what the project calls today |
| `extract_text_from_mem_by_pages` | `(&[u8]) -> Result<Vec<String>, OutputError>` |
| `extract_text_by_pages` | `(P: AsRef<Path>) -> Result<Vec<String>, OutputError>` |
| `output_doc_page` | `(&Document, &mut dyn OutputDev, page_num: u32)` |

Implementing `OutputDev` directly would go further still: `begin_page` receives the page number and `output_character` receives a glyph transform, i.e. coordinates. That is bbox-level precision with no new dependency.

Verified empirically against arXiv 1706.03762v7 (15 pages): `extract_text_from_mem_by_pages(...).join("")` is byte-identical to `extract_text_from_mem(...)` apart from a single whitespace character, because both drive the same `PlainTextOutput` engine. Page boundaries landed correctly.

One caveat worth knowing: `extract_text_by_pages` terminates on `while let Ok(...)`, so a page that fails mid-document is indistinguishable from end-of-document and silently truncates the body. Comparing the returned page count against the document's page count catches this.

### Alternatives considered and rejected

| Crate | Version | Per-page | Pure Rust | License | Verdict |
|---|---|---|---|---|---|
| pdf-extract | 0.12.0 | Yes | Yes | MIT | **Keep.** Already a dependency, already sufficient. |
| pdfium-render | 0.9.4 | Yes, plus per-char bboxes | No — needs the PDFium C++ binary | MIT/Apache-2.0 | Best extraction quality, but a native blob across five dist targets. Revisit only if extraction *quality* becomes the complaint. |
| mupdf | 0.8.0 | Yes | No | **AGPL-3.0** | Hard block: incompatible with this project's Apache-2.0 license. |
| lopdf | 0.45.0 | Page objects, no text engine | Yes | MIT | Strictly more work for strictly worse output; it is the layer beneath pdf-extract. |
| poppler-rs | 0.26.0 | Yes | No — system glib/GTK | MIT | Non-starter for a self-contained CLI on Windows. |
| pdf (pdf-rs) | 0.10.0 | Page-level | Yes | MIT | Text extraction is immature relative to pdf-extract. |

The native-dependency options deserve particular skepticism here: this project already dropped its two musl targets because the bundled ONNX runtime had no musl prebuilts. Taking on a second native dependency invites the same class of pain again.

### Page number is a weaker anchor than it looks

Worth stating plainly, because it shapes the design: a preprint's page 7 is not the published version's page 7, and neither is the page 7 of the same paper in a proceedings volume. Page numbers are stable only for the exact file that was ingested. Section headings are stable across renderings.

The section markers are, in other words, already the better anchor, and they are already in the stored body. What is missing is not the anchor but the plumbing: `paper_body` returns a truncated blob, and body FTS hits return only the paper row, so a match tells the user *which paper* without telling them *where*.

This points at the cheaper and more valuable half of the work: return the matching snippet along with its enclosing section, using SQLite FTS5's built-in `snippet()`. That delivers "here is the quote, in this section, in this paper" with no new dependency and no re-ingest.

Both halves ship together: page markers are nearly free, and the snippet plumbing is what actually delivers evidence.

### Migration

No migration is required, and none is possible without re-ingest, because page boundaries cannot be recovered from already-stored text.

Existing bodies stay valid and searchable; they simply lack page markers. Because `papers.pdf_path` is retained and `set_paper_body` is `INSERT OR REPLACE`, re-ingest is idempotent and can be done per paper. Keeping the page markers inline in the body text (rather than moving to one row per page) avoids touching the `paper_bodies` primary key, the FTS5 shadow table, and its three triggers.

Two caveats for any re-ingest path: `pdf_path` is an absolute canonicalized path, so a moved or deleted PDF must be skipped with a warning rather than treated as a failure; and the vector index embeds the body, so re-ingested papers need `index_rebuild`.

---

## Item 2: Zotero live integration

### What the importer does today

`src/adapters/bib_importer.rs` parses `.bib`/`.bibtex` through the `biblatex` crate and `.json` as CSL-JSON, dispatched by file extension in `src/application/paper_import.rs:44`. Both parsers populate exactly six fields: title, authors, year, venue, DOI, URL.

Everything else stays at `Paper::new` defaults. Most consequentially, `abstract_text` stays empty — and that field feeds both gap analysis and the search index, so imported papers land semantically empty next to ingested ones.

Dedupe is DOI-only, one lookup per paper. A paper without a DOI re-imports on every run; the module's own test asserts this behavior. There is no DOI normalization, so `10.1/X` and `https://doi.org/10.1/X` are distinct rows.

### The Zotero local API, verified

Source: [Zotero local API documentation](https://www.zotero.org/support/dev/web_api/v3/local_api).

The API is **read-write**, not read-only, which contradicts the assumption implicit in how the roadmap item was framed. Reads need no authentication; writes (Zotero 10+) require a local key that the user grants through a confirmation dialog. Changes made through it are ordinary local changes, visible in the Zotero UI immediately.

Other established facts: base URL `http://localhost:23119/api/`; the user must enable "Allow other applications on this computer to communicate with Zotero" or every request 403s; writes require a `Zotero-Server-ID` header and are gated on Zotero 10+ (released 2026-08-24, three weeks before this investigation); write keys are single-use unless the user picks "Always Allow"; the dialog is rate-limited to five requests per minute. `/items/<key>/file` redirects to a `file://` path, which is a genuinely useful way to locate attached PDFs.

No Rust crate implements the local API. The four crates found all target the cloud Web API. Since `reqwest` and `serde_json` are already dependencies, no new crate would be needed either way.

Notably, **Zotero has no reading-status or rating field.** The two fields this tool owns that Zotero does not have nowhere to sync back to except the free-text `extra` field or synthetic tags, both of which round-trip badly.

### Gate verdict: closed

The gate reads "import path proving insufficient in practice." It has not proven insufficient. It has proven incompletely implemented.

Every user-facing complaint that would motivate live sync traces back to the importer discarding data the export files already carry:

1. The abstract is dropped. `abstract` (BibTeX), `abstractNote` (Zotero JSON), and `abstract` (CSL-JSON) are all present in exports and none are read.
2. Tags are dropped. Zotero keywords are the primary way users organize, and `paper.tags` is never populated.
3. Zotero's native JSON export fails. `.json` is hardcoded to CSL, so a user who picks "Zotero JSON" in the export dialog gets a parse error.
4. DOI-less items duplicate on every re-import, which makes the re-export loop actively harmful rather than merely tedious.

Only one complaint is unique to live integration: having to re-run the export dialog to pick up new items. That is a convenience gap, not a capability gap.

Two further reasons to leave the gate closed. Two-way sync has no coherent write target, since the fields this tool owns do not exist in Zotero's schema. And the write path requires a Zotero major version released three weeks ago, so the headline feature would demand users be on a near-current release.

### If it is ever built

Minimal scope would be one-way read with no sync state: `GET /api/users/0/items` mapped to `Paper`, reusing the existing import dedupe, surfaced as `research ingest --source zotero` through the existing `PaperSource` port. Reads need no auth and no server-ID handling, so the entire authorize apparatus is skippable. One error case matters: 403, with a message naming the exact setting to enable.

What should not be built: writes and the authorize flow, sync state and delta tracking, conflict resolution, and any client-crate abstraction over what amounts to two `reqwest` calls.

The largest maintenance liability in the item is conflict resolution, and it is entirely avoidable by staying read-only. Both sides mutate independently and offline, Zotero's local versions are per-instance, and a user on two machines produces two disjoint version spaces with no merge UI.

### Risks in the item as written

- Depending on a desktop GUI's process state and a non-default checkbox is a new class of failure for a CLI that is otherwise file-and-network based.
- The local read port is unauthenticated; the Zotero documentation explicitly warns against exposing it. This stays localhost-only.
- Reads work on Zotero 7+, writes need 10+, so supporting both means runtime capability detection.
- Zotero's documentation cautions that the local implementation does not replicate every detail of the Web API, and Zotero shipped three major versions in eight months of 2026. Breakage odds are non-trivial and cannot be covered by CI against a GUI application.

---

## Decisions

**Implement the PDF work.** The blocking premise was false, so there is nothing to gate on. Per-page markers plus snippet-with-section retrieval, no new dependency, no schema change, no forced re-ingest.

**Do not build Zotero sync. Fix the importer instead.** Reading the abstract and tags that exports already carry, accepting Zotero's native JSON, and normalizing DOIs before dedupe addresses the concrete pain for a fraction of the cost. If live sync is still wanted after that, the gate reopens on evidence rather than assumption.
