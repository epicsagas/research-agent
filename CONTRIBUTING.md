# Contributing to research-agent

Thank you for your interest in contributing. This guide covers everything you need to get started.

## Prerequisites

- **Rust 1.92+** (matches `rust-version` in `Cargo.toml`)
- Git

## Development Setup

```bash
git clone https://github.com/epicsagas/research-agent
cd research-agent
cargo build
```

## Development Commands

```bash
# Run all tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Architecture

```
src/
├── domain/        # Paper, ResearchTopic, KnowledgeGap, Citation, ResearchReport
├── ports/         # PaperSource, IndexStore, ResearchEngine
├── adapters/      # ArxivSource, SemanticScholarSource, OpenAlexSource,
│                  # EuropePmcSource, ZoteroSource/Write, PdfSource, BibImporter,
│                  # SqliteStore, LlmResearchEngine
├── application/   # IngestPipeline, GapAnalyzer, ReportGenerator, enrichment
├── store/         # SQLite schema and migrations
├── mcp/           # stdio MCP server (tools, params, stdin guard)
├── dashboard.rs + # local web dashboard (embedded single-page UI)
│   dashboard/
├── composition.rs # dependency wiring
├── config.rs      # CLI configuration
├── onboard.rs     # first-run onboarding wizard
├── error.rs       # Error types
├── lib.rs         # Public API
└── main.rs        # CLI entry point
```

### Domain (`domain/`)

Core business entities: `Paper`, `ResearchTopic`, `KnowledgeGap`, `Citation`, `ResearchReport`. These types have no dependencies on infrastructure or external crates beyond `serde`.

### Ports (`ports/`)

Trait definitions for external interactions:
- `PaperSource` — fetch papers from remote APIs or local files
- `IndexStore` — persist and query papers, topics, and reading state
- `ResearchEngine` — analyze gaps and generate reports

### Adapters (`adapters/`)

Concrete implementations of port traits:
- `ArxivSource` — arXiv API client
- `SemanticScholarSource` — Semantic Scholar API client
- `SqliteStore` — SQLite + FTS5 storage backend
- `LlmResearchEngine` — LLM-backed gap analysis and report generation

### Application (`application/`)

Use-case orchestration:
- `IngestPipeline` — fetch, deduplicate, and store papers
- `GapAnalyzer` — compare coverage against topic scope
- `ReportGenerator` — compile research reports from indexed papers

## Pull Request Process

1. Fork the repository and create a branch from `main`
2. Make your changes with clear, conventional commit messages
3. Ensure `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` all pass
4. Include tests for any new logic
5. Open a pull request with a description of the change and motivation

## Coding Guidelines

- Keep the MSRV at Rust 1.92 — avoid features introduced after that version
- New adapters must implement the corresponding port trait
- Each PR should include tests for any new logic
- Run `cargo clippy -- -D warnings` and `cargo fmt` before submitting

## Reporting Issues

- **Bug reports**: Use the Bug Report issue template
- **Feature requests**: Use the Feature Request issue template
- **Security vulnerabilities**: See [SECURITY.md](SECURITY.md)

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 license of this repository ([LICENSE](LICENSE)).
