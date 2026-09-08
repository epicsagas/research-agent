# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Zotero live integration

Two-way sync with a running Zotero instance (local HTTP API) instead of the
current export-import flow (`research import <file|dir>`). Still gated, but the
gate moved: the importer used to discard abstracts, tags, and Zotero-native
JSON that the export files already carried, so the export-import path had never
actually been evaluated at full strength. That is fixed, and the gate now reads:
build this once the repaired import path demonstrably falls short in practice.

Two findings worth keeping (see
[docs/roadmap-research-2026-09.md](docs/roadmap-research-2026-09.md)). The local
API does support writes, not just reads, so two-way sync is technically
possible; but Zotero has no reading-status or rating field, so the two fields
this tool owns have nowhere to sync back to. If it is ever built, the useful
scope is one-way read as another `--source`, not a sync engine: reads need no
auth, while writes drag in single-use keys, per-instance version spaces, and
conflict resolution with no merge UI.
