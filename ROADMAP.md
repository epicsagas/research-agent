# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Zotero write-back (two-way sync)

The one-way read half shipped (`research ingest --source zotero`, reads the
running instance's local API, needs no auth) and the importer deficiencies
that used to motivate live sync are fixed: abstracts, tags, Zotero-native
JSON, API-shaped items, and DOI normalization all landed. What remains is
deliberately unbuilt: writing back to Zotero, or full two-way sync. The local
API's write path requires per-instance single-use keys the user grants through
a confirmation dialog, and any merge needs conflict resolution with no UI to
resolve it. Zotero also has no reading-status or rating field, so the two
fields this tool owns have nowhere to sync back to (full analysis:
[docs/roadmap-research-2026-09.md](docs/roadmap-research-2026-09.md)). Gated
on a concrete need: a user wanting annotations or edits pushed from this tool
into their Zotero library.

## Performance benchmark harness

The eval quality gate runs correctness (153 tests) and lint (`clippy -D
warnings`) on every change, but the performance dimension is disabled because
no criterion benchmark exists. Deliberate: recent work is I/O and config
flows, not hot paths, and there is no latency complaint to tune against. Add
a `benches/` criterion harness measuring query latency (lexical vs hybrid)
and index build throughput when either becomes user-visible; wire it into
`.harness` `eval.yaml` at that point.

## PDF extraction quality (pdfium-render)

Extraction stays on `pdf-extract` (pure Rust, no native blob). The recorded
alternative is pdfium-render: best-in-class extraction quality, but it drags
the PDFium C++ binary across five release targets, the same class of native
dependency pain that already cost this project its musl builds. Gated on
extraction *quality* becoming the actual complaint, not page anchoring (that
shipped; see the PDF evidence entry in the research doc for the
crate-by-crate comparison).
