# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Support / dispute citation context

Classify citations as supporting, disputing, or mentioning (Scite-style).
Semantic Scholar's Graph API exposes free per-citation `intents`
(methodology/background/result) and `isInfluential`, so this is now an
engineering task on the existing SemanticScholarSource rather than a blocked
data strategy; open questions are intent coverage breadth and rate limits.

## Zotero live integration

Two-way sync with a running Zotero instance (local HTTP API) instead of the
current export-import flow (`research import <file|dir>`). Gated on the
import path proving insufficient in practice.

## Page-level evidence for PDF bodies

The stored body text keeps section headings (`## Section` markers), but
pdf-extract does not expose page boundaries. Page-anchored citations need a
different PDF extraction layer.
