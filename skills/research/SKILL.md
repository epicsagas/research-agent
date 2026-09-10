---
name: research
description: "Academic literature research via the `research` CLI — ingest papers from arXiv / Semantic Scholar / local PDFs into a topic hierarchy, run LLM gap analysis and report generation, track reading status and 1-5 ratings, and full-text search. Triggers on: research papers, literature review, find papers on, arxiv search, semantic scholar, research gaps, 논문 리서치, 문헌 탐색, 리서치."
---

# Research — Academic Literature Assistant

Drive the `research` CLI (installed at `~/.cargo/bin/research`) to collect, organize, and analyze academic papers, then report findings in Korean.

## Workspace Setup

Run once per research workspace (keep it fixed, e.g. `~/research-ws`):

```bash
mkdir -p ~/research-ws && cd ~/research-ws
research init
```

For real LLM gap analysis and reports, edit `research.toml` and add:

```toml
[llm]
provider = "anthropic"            # or "openai"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY" # name of the env var holding the key
```

Without `[llm]`, `gaps`/`report` return deterministic stub output.

All `research` commands accept `--db <PATH>` (default `~/.research/research.db`). Pass `--db` explicitly (or set it in `~/.research/config.toml`) to use a workspace-local database.

## Core Workflow

### 1. Topics (organize the research)

```bash
research topics add "Diffusion Models" --description "generative modeling"
research topics add "Score-Based" --parent <parent-id>   # sub-topic, depth auto
research topics list                                     # parents-first, indented by depth
```

### 2. Ingest papers

```bash
research ingest "<query>" --source arxiv --limit 20 [--topic <id>]
research ingest "<query>" --source s2 --limit 20         # Semantic Scholar
research ingest "<query>" --source all --limit 20        # both
research ingest --source pdf --path ./papers/            # local PDFs (file or dir)
```

`--topic <id>` auto-links every ingested paper to the topic; a missing topic id is an error (not a silent skip). Capture paper ids from the output for rating/reading later.

### 3. Analyze gaps (LLM)

```bash
research gaps                       # all topics
research gaps --topic <id>          # one topic
```

Requires `[llm]` config. Returns `KnowledgeGap` records (missing literature, methodology gaps, etc.).

### 4. Read & rate

```bash
research read <paper-id>                              # show detail (status, rating, abstract)
research read <paper-id> --status completed           # unread|queued|in_progress|completed|abandoned
research read <paper-id> --rating 4                   # 1-5
research read <paper-id> --status read --rating 5     # both validated before any write
```

Rating is enforced 1-5; out-of-range bails before touching the DB.

### 5. Generate report (LLM)

```bash
research report --topic <id1>,<id2> --title "Diffusion Survey"
```

### 6. Search

```bash
research query "consistency models" --limit 10        # FTS5 trigram over title/abstract/notes/tags/keywords + body
```

### 7. Enrich (make paraphrased queries findable)

Search is FTS5 full-text: it matches words the paper actually contains. Keywords cover the words it does not — the synonym, the expanded acronym, the phrasing someone would search for first. Without them, a query for "transformer" misses a paper whose abstract only says "attention mechanisms".

```bash
research enrich --missing --limit 20      # uses [llm] if configured
research enrich <ID> --keywords "kw1; kw2; kw3"   # mechanical write, no LLM
```

**When there is no `[llm]` provider, you do this yourself** — that is the normal path for an agent, not a fallback. `research enrich` (or the `papers_missing_keywords` MCP tool) prints papers with no keywords yet. For each: read the title and abstract, then write 5-10 English keywords covering synonyms, expanded acronyms, broader field terms, and alternative phrasings. Do not repeat words already in the title or abstract — FTS5 already indexes those. Store each with `enrich_paper` (MCP) or `research enrich <ID> --keywords "..."`.

Run this after any ingest. Newly ingested papers have no keywords.

### 8. Dashboard (human view)

```bash
research dashboard    # local web UI at http://127.0.0.1:7777/ (read-only; Ctrl-C stops)
```

Point the user here when they want to browse the library themselves; it shows counts, the topic tree, and reading status. Not an agent tool — the CLI/MCP already covers programmatic access.

## Process

1. **Bound the request** — what topic, what depth (survey / specific question), which sources.
2. **Ensure workspace** — `cd ~/research-ws` or create + `research init`; verify `[llm]` if gaps/report needed.
3. **Create/locate topic** — `topics add` (with `--parent` for sub-areas); note the id.
4. **Ingest** — `ingest --topic <id>` so papers link automatically.
5. **Enrich** — `enrich --missing` (generate the keywords yourself when no `[llm]` is set) so later queries survive paraphrasing.
6. **Triage** — `query` / `read` to skim; set `--status` and `--rating` for the relevant ones.
7. **Analyze** — `gaps --topic <id>` to find holes.
8. **Synthesize** — `report --topic <id>` for a written summary.
9. **Report to the user in Korean** — summarize: 수집 논문 수, 핵심 논문(제목/평점), 식별된 갭, 보고서 경로.

## Anti-Rationalization

| Excuse | Rebuttal | Instead |
|---|---|---|
| "I'll just list papers from memory" | Stale/hallucinated citations | Always `research ingest` from real sources |
| "Skip the topic, just dump papers" | Unorganized, can't gap-analyze | Create a topic, link with `--topic` |
| "Rating/status is cosmetic" | Drives `status` overview + triage | Set `--status`/`--rating` as you read |
| "gaps/report without `[llm]` is fine" | Returns stub, not real analysis | Configure `[llm]` or tell the user it's stubbed |

## Evidence Required

- [ ] Workspace exists (`research init` ran, `research.db` present)
- [ ] At least one topic created
- [ ] Papers ingested from a real source (arxiv/s2/pdf)
- [ ] Final summary in Korean with concrete paper titles + counts

## Red Flags

- Ingesting without `--topic` when the user wants organized research
- Running `gaps`/`report` without `[llm]` and presenting stub output as real analysis
- Citing papers that aren't in the local index (no `research query` hit)
- Reporting in English when the user-facing summary should be Korean
