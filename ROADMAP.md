# Roadmap

Shipped capabilities live in the [README](README.md) and
[CHANGELOG](CHANGELOG.md); this file tracks what is deliberately **not** built
yet. Items are ordered by expected value, not scheduled.

## Citation graph

Paper-to-paper relationships (references, citations, related work) via the
Semantic Scholar citations API or OpenAlex `referenced_works`. Powers "what
should I read next from this paper" and coverage views that follow edges, not
just keywords.

## Support / dispute citation context

Classify citations as supporting, disputing, or mentioning (Scite-style).
Requires a citation-classification data source; none is freely and completely
available yet, so this waits on a data strategy rather than engineering.

## Domain sources

PubMed / Europe PMC and bioRxiv-style preprint servers as additional
`PaperSource` adapters. The adapter pattern (`src/ports/paper_source.rs`)
makes each a small, self-contained addition; they are gated on a concrete
need from biomedical research workflows.

## Zotero live integration

Two-way sync with a running Zotero instance (local HTTP API) instead of the
current export-import flow (`research import <file|dir>`). Gated on the
import path proving insufficient in practice.

## Page-level evidence for PDF bodies

The stored body text keeps section headings (`## Section` markers), but
pdf-extract does not expose page boundaries. Page-anchored citations need a
different PDF extraction layer.
