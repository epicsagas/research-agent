# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Zotero two-way sync (beyond tag write-back)

Both read and the first write slice shipped: `research ingest --source zotero`
reads the running instance's local API (no auth), and
`research export --to zotero` pushes paper tags back, matched by normalized
DOI (dry-run by default; `--apply` writes, per-item confirmation dialogs
authorize it). What remains deliberately unbuilt is full two-way sync of
content: abstracts, notes, or child-item edits. The local API's write path
requires per-instance single-use keys the user grants through a confirmation
dialog, any merge needs conflict resolution with no UI to resolve it, and
Zotero has no reading-status or rating field, so the two fields this tool owns
have nowhere to sync back to (full analysis:
[docs/roadmap-research-2026-09.md](docs/roadmap-research-2026-09.md)). Gated
on a concrete need: a user wanting annotations or edits pushed from this tool
into their Zotero library.

## Performance regression gating

A criterion harness exists (`benches/core.rs`: lexical query, FTS index
rebuild, body-evidence snippet over a deterministic 500-paper corpus, so no
network is involved). The eval gate
runs it and reports medians, but **informationally**: absolute thresholds are
machine-dependent and shared CI runners swing 10-30%, so timing never turns
the gate red. What is deliberately not built: same-machine baseline diffing
with a regression threshold (e.g. `critcmp` against a saved baseline on a
pinned reference machine). Add it when a latency complaint exists and a
reference machine is designated.

## PDF extraction quality (pdfium-render)

Decided against (2026-09-10 council, unanimous). Extraction stays on
`pdf-extract` (pure Rust, no native blob): pdfium-render would drag the
PDFium C++ binary across five release targets, the same class of native
dependency pain that already cost this project its musl builds, with zero
recorded quality complaints to justify it. If a real quality corpus ever
exists (multi-column layouts, ligatures, text pdf-extract provably loses),
the cheaper path comes first: implement `pdf_extract::OutputDev` directly for
bbox-precision output with no new dependency. Only if that falls short does
pdfium-render reopen, feature-gated with dynamic loading and a pdf-extract
fallback.
