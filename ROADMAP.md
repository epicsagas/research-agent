# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Support / dispute citation context

Classify citation edges as supporting, disputing, or mentioning
(Scite-style). The engineering side is in place: the citation graph syncs
both directions and the `citations` table already carries an unused
`context` column that can hold the label. The data piece is Semantic
Scholar's free per-citation `intents` (methodology/background/result) and
`isInfluential`, reachable through the existing SemanticScholarSource; open
questions are intent coverage breadth and unauthenticated rate limits,
since classifying a paper's citing works costs one request per edge.

## Zotero live integration

Two-way sync with a running Zotero instance (local HTTP API) instead of the
current export-import flow (`research import <file|dir>`). Gated on the
import path proving insufficient in practice.

## Page-level evidence for PDF bodies

The stored body text keeps section headings (`## Section` markers), but
pdf-extract does not expose page boundaries. Page-anchored citations need a
different PDF extraction layer.
