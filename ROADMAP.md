# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Zotero two-way sync

What remains of the Zotero item after the read half shipped: writing back to
Zotero, or full two-way sync. The useful scope turned out to be one-way read,
which now exists as `research ingest --source zotero` (reads the running
instance's local API, needs no auth). Writes are the part deliberately not
built: the local API's write path requires per-instance single-use keys the
user grants through a confirmation dialog, and any merge needs conflict
resolution with no UI to resolve it. Zotero also has no reading-status or
rating field, so the two fields this tool owns have nowhere to sync back to
(see [docs/roadmap-research-2026-09.md](docs/roadmap-research-2026-09.md)).
Gated on a concrete need: a user wanting annotations or edits pushed from
this tool into their Zotero library.
