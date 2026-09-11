<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> 이 문서는 [README.md](../../../README.md)의 번역본입니다.
> 영어 원문이 공식 문서(source of truth)이며 최신 내용을 담고 있을 수 있습니다.

<div align="center">

# research-agent

> AI 에이전트를 위한 장기 연구 메모리 — 논문 색인, 지식 공백 분석, 서베이 리포트 생성

<p align="center">
  <a href="https://github.com/epicsagas/research-agent/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=ffd700&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/issues"><img alt="Issues" src="https://img.shields.io/github/issues/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=ff6b6b&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/commits/main"><img alt="Last commit" src="https://img.shields.io/github/last-commit/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=58a6ff&logo=git&logoColor=white" /></a>
</p>
<p align="center">
  <a href="https://github.com/epicsagas/research-agent/releases"><img alt="Version" src="https://img.shields.io/github/v/release/epicsagas/research-agent?style=for-the-badge&labelColor=0d1117&color=fc8d62&logo=github&logoColor=white" /></a>
  <a href="https://github.com/epicsagas/research-agent/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/epicsagas/research-agent/total?style=for-the-badge&labelColor=0d1117&color=3498db&logo=github&logoColor=white" /></a>
  <a href="../../../LICENSE"><img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-3fb950?style=for-the-badge&labelColor=0d1117" /></a>
</p>

<p align="center">
  <a href="../../../README.md">English</a> |
  <b>한국어</b> |
  <a href="../ja/README.md">日本語</a> |
  <a href="../zh-Hans/README.md">简体中文</a> |
  <a href="../zh-Hant/README.md">繁體中文</a> |
  <a href="../es/README.md">Español</a> |
  <a href="../fr/README.md">Français</a> |
  <a href="../de/README.md">Deutsch</a> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## 이 프로젝트는 무엇인가요?

research-agent는 **AI 에이전트를 위한 연구 서버**입니다. 논문(arXiv, Semantic Scholar, 로컬 PDF)을 로컬 SQLite 라이브러리에 색인하고, 에이전트가 MCP(Model Context Protocol)를 통해 직접 구동합니다. 논문 수집, 검색, 읽기 진행 상황 추적, 자체 모델을 통한 커버리지 공백(coverage gap) 분석, 서베이 리포트 작성을 모두 지원합니다. 별도의 API 키는 필요하지 않습니다.

동일한 바이너리가 터미널 및 스크립트를 위한 독립형 CLI로도 동작합니다.

## Web UI

<img width="49%" src="../../../assets/overview.png" alt="dashboard-overview" />
<img width="49%" src="../../../assets/papers.png" alt="dashboard-overview" />

## 주요 기능

| | 기능 | 가치 |
|--|------|------|
| 🤖 | MCP 서버 | 에이전트가 stdio를 통해 직접 구동하는 18개 도구 — API 키 불필요 |
| 🧠 | 에이전트 네이티브 분석 | 지식 공백 분석과 리포트 작성이 에이전트 내부에서 실행됨: 도구가 구조화된 상태를 전달하고, 에이전트가 추론하며, 결과가 영구 저장됨 |
| 🔗 | 인용 그래프 | OpenAlex 기반 논문 간 참조 엣지(정방향 및 역방향), 선택적으로 Semantic Scholar 인용 의도(citation intent) 라벨링 |
| 📚 | 논문 색인 | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), bioRxiv 계열 프리프린트, 로컬 PDF, Zotero(실행 중인 인스턴스 또는 내보내기 파일) |
| 🔍 | 하이브리드 검색 | FTS5 어휘 검색 + 로컬 ONNX 시맨틱 검색, RRF 융합 — 오프라인 동작 |
| 📂 | 토픽 트리 | 하위 토픽을 통한 계층적 연구 정리 |
| 📖 | 읽기 추적기 | 읽을 목록 큐 관리, 진행 상황 추적 및 평점 부여 |
| ⚡ | 단일 바이너리 | 런타임 불필요, 별도 서버 불필요 — `research` 하나로 동작 |

## 빠른 시작

AI 에이전트에 플러그인으로 설치하세요 — Rust 툴체인이 필요하지 않습니다:

### Claude Code

```bash
claude plugin marketplace add epicsagas/research-agent
claude plugin install research-agent@research-agent
```

### Codex

```bash
codex plugin marketplace add epicsagas/research-agent
codex plugin add research-agent@research-agent
```

### Antigravity (agy)

```bash
agy plugin install https://github.com/epicsagas/research-agent
agy plugin enable research-agent
```

### Grok Build

```bash
grok plugin install epicsagas/research-agent --trust
```

### Hermes Agent

```bash
hermes plugins install https://github.com/epicsagas/research-agent
hermes plugins enable research-agent
```

Hermes는 루트 `plugin.yaml`과 `__init__.py`의 `register(ctx)`를 로드합니다. Hermes는 MCP를 지원하지 않으므로, 에이전트는 번들된 스킬의 CLI 명령어를 통해 `research`를 구동합니다. 먼저 바이너리를 설치하세요(`brew install epicsagas/tap/research-agent` 또는 curl 설치 스크립트). SessionStart 자동 설치 훅은 hermes에 적용되지 않습니다. skills_guard가 설치 검사를 차단하는 경우 hermes 설정에서 `plugins.scan_on_install: false`로 설정하세요.

플러그인은 세션 시작 시 `research` 바이너리를 자동 설치하고 18개의 MCP 도구를 노출하므로, 에이전트가 자체 모델을 사용하여 논문 수집, 검색, 분석, 리포트 작성을 스스로 수행할 수 있습니다.

설치 후 에이전트에게 다음과 같이 요청해 보세요:

- "arXiv에서 그래프 신경망(GNN) 학습에 관한 최신 논문들을 수집해 줘"
- "내 <topic> 커버리지에 어떤 지식 공백이 남아 있어?"
- "<topic>에 대한 서베이 리포트를 생성해 줘"

이 요청들이 도구 수준에서 어떻게 처리되는지 보면:

| 이렇게 요청하면 | 에이전트는 도구를 이렇게 엮습니다 |
|---------|------------------|
| 토픽을 처음부터 끝까지 조사 | `init` → `topic_add` → `ingest`(arXiv/S2, 토픽에 연결) → `topic_brief` → 에이전트가 자체 모델로 브리프를 분석 → `gaps_record` → 기록된 공백을 겨냥해 다시 `ingest` → `report_material` → `report_save` |
| 이미 모아둔 논문 찾기 | `query_papers` → `paper_body(id, query=...)` 해당 구절을 섹션·페이지와 함께 인용 |
| 읽은 것 추적 | `update_read`(상태, 1-5 평점); 커버리지 개요는 `state` |
| 대시보드 띄우기 | Bash: `research dashboard` 실행 후 http://127.0.0.1:7777 이 떴다고 알려 줌 |

모든 작업은 로컬 SQLite 라이브러리 하나(기본값 `~/.research/research.db`)에서 이뤄집니다. 프로젝트 전용 라이브러리가 필요하면 호스트의 MCP 설정에서 서버가 다른 파일을 가리키게 하면 됩니다(`"args": ["mcp", "--db", "./project.research.db"]`, `--db`는 전역 플래그). 아니면 에이전트가 스킬/CLI 호출에 `--db`를 붙이게 해도 됩니다.

## 에이전트의 구동 방식

플러그인은 세션 시작 시 `research` 바이너리를 자동 설치하고 **stdio MCP 서버**(`research mcp`)를 실행합니다. 에이전트가 도구를 직접 검색하고 호출하므로 사용자가 직접 CLI 명령을 입력할 필요가 없습니다.

**도구 목록** (18개): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`.

분석은 **에이전트 네이티브** 방식으로 이루어집니다. `topic_brief`와 `report_material`이 구조화된 라이브러리 상태(논문, 읽기 진행 상황, 기록된 공백, 커버리지)를 전달하면, 에이전트가 자체 모델로 추론하고 `gaps_record` / `report_save`가 분석 결과를 저장합니다. 본문 검토 시 `paper_body` 도구에 선택적 `query` 파라미터를 전달하여 전체 본문 대신 섹션 및 페이지가 표시된 증거 스니펫을 반환받아 컨텍스트를 크게 절약할 수 있습니다. MCP 워크플로 전체에서 `[llm]` 설정이나 API 키가 전혀 필요하지 않습니다.

원시 JSON-RPC를 통해 서버 스모크 테스트를 실행할 수 있습니다:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

`mcp` cargo 기능은 **기본적으로 켜져 있습니다**. CLI 전용 빌드를 원하면 `cargo build --no-default-features`를 사용하세요.

## 독립형 CLI (보조 기능)

직접 명령어를 실행하는 것을 선호하시나요? 동일한 바이너리를 일반 CLI로 사용할 수 있습니다.

```bash
# macOS / Linux — 사전 빌드된 바이너리, Rust 설치 불필요
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — 사전 빌드된 바이너리, Rust 설치 불필요
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### 대시보드 사용하기

같은 데이터베이스를 보여 주는 화면입니다. 터미널에서 띄우고 브라우저로 보세요:

```bash
research dashboard        # 실행 후 http://127.0.0.1:7777 접속 (루프백 전용)
```

| 화면 | 보여 주는 것 |
|--------|---------------|
| Overview | 라이브러리 규모, 읽기 깊이, 파이프라인 퍼널, 토픽 커버리지 |
| Papers | 라이브러리 테이블. 행을 클릭하면 상세 보기, 내장 리더로 페이지 단위 본문이나 저장된 PDF 열람, 읽기 상태와 평점 수정 |
| Pipeline | 발견부터 정독까지 각 논문이 어디쯤 와 있는지 |
| History | 지금까지 일어난 일의 최신순 피드 |
| Results | 지식 공백과 생성된 리포트 |
| Config | `[llm]`, 워크스페이스 경로, 대시보드 바인드 설정 편집 |

일반적인 흐름: Overview에서 토픽 커버리지를 확인하고, Results에서 기록된 공백을 살펴본 뒤, 터미널로 돌아가 그 공백을 겨냥해 다음 수집을 돌리고, Papers에서 읽기 진행 상황을 추적합니다.

### 최초 실행 온보딩

터미널에서 `research init`을 실행하면 핵심 설정들을 대화형으로 안내합니다: 데이터베이스 위치, 공백 분석 및 리포트 작성에 사용할 LLM 제공자(키 자체는 지정한 환경 변수에서 읽어오며 설정 파일에 저장되지 않음), 하이브리드 검색을 위한 임베딩 모델(모델 즉시 다운로드 옵션 포함). 로컬 서버(Ollama, LM Studio)에서 이미 로드된 모델을 탐색하여 실제 존재하는 모델 중에서 선택할 수 있습니다. 다시 실행해도 안전합니다: 기존 값이 기본값이 되며 아무것도 초기화되지 않습니다.

설정 가능한 모든 항목은 `~/.research/config.toml`에 나타납니다. 설정하지 않은 옵션은 기본값과 함께 주석 처리되어 파일 자체가 문서 역할을 합니다. `research init --no-onboard`는 대화형 프롬프트를 건너뜁니다(stdin이 터미널이 아닐 때도 자동으로 건너뛰므로 스크립트가 차단되지 않습니다).

## 업데이트 방법

| 설치 방법 | 명령어 |
|-----------|--------|
| curl 설치 스크립트 (macOS/Linux) | 위의 설치 스크립트 재실행 |
| PowerShell 설치 스크립트 (Windows) | 위의 설치 명령어 재실행 |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

설치된 버전 확인:

```bash
research --version
```

## 명령어 목록

| 명령어 | 설명 |
|--------|------|
| `research init` | 연구 워크스페이스 초기화 (터미널 대화형 온보딩: 제공자, 환경 변수 키 이름, 임베딩 모델 + 다운로드; `--no-onboard`로 건너뛰기) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), 프리프린트 서버(bioRxiv, medRxiv 등)에서 논문 수집; 선택적으로 특정 토픽에 바로 연결 |
| `research ingest [--source zotero] [query] [--topic <id>]` | 로컬 API를 통해 실행 중인 Zotero에서 논문 읽기(`ZOTERO_BASE_URL`로 엔드포인트 재정의 가능, "Allow other applications on this computer to communicate with Zotero" 활성화 필요); 쿼리가 없으면 전체 라이브러리 가져옴. `--source all`에는 포함되지 않음 — 개인 라이브러리는 발견 소스가 아님 |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | 로컬 PDF 파일 수집 (본문 전체 텍스트가 저장되어 검색 가능) |
| `research import <file\|dir>` | BibTeX/BibLaTeX, CSL-JSON 또는 Zotero JSON 파일(데스크톱 내보내기 또는 API 형식) 가져오기(초록, 태그, 정규화된 DOI 포함); 이미 라이브러리에 있는 논문은 건너뜀 |
| `research index [--rebuild]` | 검색 인덱스 생성 또는 재생성 (FTS + 벡터 인덱스) |
| `research reingest [--missing-pages]` | 저장된 PDF 본문 재추출 (페이지 마커가 생기기 전에 수집된 본문에 마커 추가) |
| `research query <q> [--evidence]` | 논문 검색 — 임베딩 사용 가능 시 어휘+시맨틱 하이브리드 검색; `--evidence` 사용 시 섹션 및 페이지와 함께 일치하는 본문 텍스트 표시 |
| `research references <id> [--cited-by] [--intents]` | OpenAlex에서 인용 그래프 엣지 가져오기; `--cited-by`는 방향 반전, `--intents`는 Semantic Scholar 인용 의도 라벨 추가 |
| `research gaps [--topic <id>]` | 지식 공백 분석 (CLI: 설정된 경우 `[llm]` 사용) |
| `research report --topic <id>` | 연구 서베이 리포트 생성 (CLI: 설정된 경우 `[llm]` 사용) |
| `research topics list` | 모든 토픽 목록 표시 |
| `research topics add <name> [--parent <id>]` | 새 토픽 추가 (`--parent`를 지정하여 하위 토픽 생성) |
| `research read <id> [--status <status>] [--rating <1-5>]` | 읽기 상태 또는 평점 업데이트 |
| `research read <id> --body` | 논문의 저장된 본문 텍스트 출력 |
| `research status` | 연구 상태 개요 표시 |
| `research mcp` | stdio MCP 서버 시작 (별칭: `serve`) |

모든 하위 명령은 전역 플래그 `--db <path>`를 지원하여 `~/.research/research.db` 대신 특정 데이터베이스를 지정할 수 있습니다(격리된 작업 공간이나 테스트 환경에 유용).

### 활용 워크플로 예시

**조사 루프: 수집하고, 공백을 찾고, 메우기**

```bash
research init                                          # 최초 1회: DB + LLM 설정
research topics add "Graph DB internals"               # 토픽 ID가 출력됩니다
research ingest "latch-free graph database" --topic <TOPIC_ID> --limit 20
research gaps --topic <TOPIC_ID>                       # 조사에서 비친 구멍
# 공백 분석이 짚어 준 부분을 겨냥해 다음 수집을 돌립니다:
research ingest "MVCC snapshot isolation graph store" --topic <TOPIC_ID>
research status                                        # 토픽별 논문·커버리지·공백
```

`research gaps`와 `research report`는 온보딩에서 설정한 `[llm]` 제공자를 사용합니다. 설정이 없으면 실패 대신 placeholder를 반환합니다.

**이미 수집된 것 찾기**

```bash
research query "latch-free transaction" --evidence     # 본문까지 검색, 섹션·페이지 표시
research read <PAPER_ID> --body                        # 저장된 본문 전문 출력
research read <PAPER_ID> --status completed --rating 5
```

**프로젝트별 라이브러리**

```bash
research --db ./project.research.db init
research --db ./project.research.db ingest "your topic" --topic <TOPIC_ID>
research dashboard --db ./project.research.db
```

## 요구 사항

- Rust 1.92+ (소스에서 빌드할 때만 필요; 사전 빌드된 바이너리는 필요 없음)
- 데이터 저장 위치: `~/.research/` (`research.db` 인덱스, `config.toml`, `embeddings.idx`)
- Linux 바이너리는 glibc ≥ 2.38(Ubuntu 24.04+)을 대상으로 합니다. 번들된 ONNX 런타임에 musl 사전 빌드가 없기 때문에 musl은 빌드되지 않습니다.
- 선택 사항: **CLI에서만** `research gaps` / `research report`에 사용할 LLM — `~/.research/config.toml`에 `[llm]` 설정(`provider`, `model`, `api_key_env`); 키는 호출 시 해당 환경 변수에서 읽어옵니다. MCP 플로우에서는 필요하지 않습니다.
- 선택 사항: `~/.research/config.toml`의 `[search]`를 통한 하이브리드 검색 튜닝 — `provider = "local"`(기본값, 번들된 ONNX 모델) 또는 `"openai"`(`openai_api_key_env` 포함); `model`로 로컬 모델 선택(기본값 `BGESmallENV15`, CPU에서 실행되는 작은 384차원 모델). 의도적으로 아직 구현되지 않은 기능은 [ROADMAP.md](../../../ROADMAP.md)를 참조하세요.

## 기여하기

[CONTRIBUTING.md](../../../CONTRIBUTING.md) 및 [ROADMAP.md](../../../ROADMAP.md)를 참조하세요. 풀 리퀘스트(PR)를 환영합니다.

## 라이선스

[APACHE-2](../../../LICENSE).
