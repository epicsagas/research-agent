# Roadmap

Direction: keep the differentiation axis — **your agent's long-term research
memory** — and raise the table stakes (sources, search, import) to parity with
established tools, mostly by reusing [llm-kernel](https://github.com/epicsagas/llm-kernel).
The design identity stays intact: single binary, local-first, offline-capable.

Analysis flows through the agent, not a bundled LLM: tools return structured
state (`topic_brief`, `report_material`), the host model reasons, findings are
persisted (`gaps_record`, `report_save`).

## v0.2.0 — Bring your library

New users don't start from an empty library.

- **OpenAlex source** — free, generous API; widens coverage beyond arXiv/S2 at
  the lowest possible cost
- **BibTeX import** — absorb an existing `.bib` file into the topic tree
- **Zotero import** — read a Zotero export/SQLite and map collections to topics

These are the on-ramp: everything else gets more valuable once an existing
library can move in.

## v0.3.0 — Memory that understands meaning

- **Hybrid search** — BM25 (today) + vector similarity via an llm-kernel
  feature, fused so either path can answer alone
- Local embeddings, no external calls — the index stays a single SQLite file

## v0.4.0 — Deeper papers

- **Section-aware PDF parsing** — keep section structure instead of flat text
- **Page-anchored evidence** — quotes carry back-references so the agent can
  cite where a claim came from

## Later — citation graph and beyond

- **Citation graph** — ingest references/related work, walk the graph from the
  library side
- **Contextual citation signals** — how a paper is cited (supporting /
  contrasting), Scite-style, when a usable open source of citation contexts
  exists
- **PubMed source** — for biomedical topics

## Non-goals

- Bundling an LLM — analysis belongs to the calling agent
