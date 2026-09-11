<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> 本文件為 [README.md](../../../README.md) 的繁體中文翻譯版本。
> 英文原版為官方權威來源（source of truth），內容可能更新更即時。

<div align="center">

# research-agent

> AI Agent 的長期學術研究記憶庫 — 索引論文、發掘知識盲區、生成綜述報告

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
  <a href="../ko/README.md">한국어</a> |
  <a href="../ja/README.md">日本語</a> |
  <a href="../zh-Hans/README.md">简体中文</a> |
  <b>繁體中文</b> |
  <a href="../es/README.md">Español</a> |
  <a href="../fr/README.md">Français</a> |
  <a href="../de/README.md">Deutsch</a> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## 這是什麼？

research-agent 是一款 **專為 AI Agent 打造的學術研究伺服器**：它將論文（arXiv、Semantic Scholar、本機 PDF 等）索引至本機 SQLite 文獻庫中，您的 Agent 透過 MCP（Model Context Protocol）即可直接驅動 —— 執行論文擷取、檢索、追蹤閱讀進度、藉由 Agent 自帶模型分析知識盲區（coverage gap），並產出歸檔綜述報告。完全無需配置第三方 API 金鑰。

同一個二進位執行檔亦可作為面向終端機與自動化腳本的獨立 CLI 工具。

## 功能特色

| | 特色 | 價值 |
|--|------|------|
| 🤖 | MCP 伺服端 | 18 個工具供 Agent 直接透過 stdio 呼叫 — 無需額外 API 金鑰 |
| 🧠 | Agent 原生分析 | 知識盲區分析與報告生成於 Agent 內部執行：工具提供結構化狀態，Agent 負責推理並持久化結果 |
| 🔗 | 引文圖譜 | 基於 OpenAlex 取得論文間正向/反向引用關係，支援 Semantic Scholar 引用意圖（citation intent）標籤 |
| 📚 | 論文索引 | 支援 arXiv、Semantic Scholar、OpenAlex、Europe PMC (PubMed)、bioRxiv 等預印本、本機 PDF 以及 Zotero（執行中的用戶端或匯出檔案） |
| 🔍 | 混合搜尋 | FTS5 詞彙搜尋 + 本機 ONNX 語意向量搜尋，RRF 融合重排 — 完全支援離線執行 |
| 📂 | 主題樹結構 | 藉由子主題階層化組織研究脈絡 |
| 📖 | 閱讀追蹤器 | 排程管理、進度追蹤與文獻評分 |
| ⚡ | 單一二進位檔 | 無額外執行環境，無需常駐背景伺服器 — 僅需單一 `research` |

## 快速上手

作為外掛程式安裝至您的 AI Agent — 無需 Rust 工具鏈：

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

Hermes 會載入根目錄下的 `plugin.yaml` 與 `__init__.py` 中的 `register(ctx)`。由於 Hermes 不支援 MCP，Agent 會透過內建 Skill 的 CLI 命令呼叫 `research` — 請先安裝二進位檔案（`brew install epicsagas/tap/research-agent` 或 curl 安裝腳本）；SessionStart 自動安裝 Hook 不適用於 Hermes。若 skills_guard 攔截了安裝檢查，請於 Hermes 設定中加入 `plugins.scan_on_install: false`。

此外掛程式會在工作階段啟動時自動安裝 `research` 二進位檔並提供 18 個 MCP 工具，讓 Agent 能利用自身模型自主擷取、檢索、分析並撰寫報告。

安裝完成後，您可以向 Agent 提出如下請求：

- "從 arXiv 擷取有關圖神經網路（GNN）訓練的最新論文"
- "我目前對 <topic> 的研究覆蓋中還缺少哪些知識？"
- "為 <topic> 產出一份文獻綜述報告"

## Agent 的使用方式

外掛程式會在工作階段啟動時自動安裝 `research` 二進位檔並啟動 **stdio MCP 伺服端**（`research mcp`）—— Agent 會自動探索並直接呼叫工具，使用者無需手動輸入 CLI 指令。

**可用工具**（共 18 個）: `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`。

分析流程採用 **Agent 原生架構**：`topic_brief` 與 `report_material` 負責傳遞結構化的文獻庫狀態（論文列表、閱讀進度、記錄的缺口、覆蓋率），Agent 藉由自身模型進行推理，再透過 `gaps_record` / `report_save` 永久儲存結果。在審閱內文時，`paper_body` 工具可接收選填的 `query` 參數，回傳附帶章節與頁碼定位的證據片段，而非整份全文，進而大幅節省上下文 Token。在整個 MCP 流程中完全無需 `[llm]` 設定或 API 金鑰。

可透過原始 JSON-RPC 對伺服器進行冒煙測試：

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

`mcp` cargo 功能 **預設啟用**；如需建置純 CLI 版本，請執行 `cargo build --no-default-features`。

## 獨立命令列工具（CLI）（輔助功能）

偏好手動於終端機操作？同一個二進位檔案可直接作為標準 CLI 使用。

```bash
# macOS / Linux — 預先編譯二進位檔，無需 Rust
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — 預先編譯二進位檔，無需 Rust
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### 首次執行引導

於終端機中執行 `research init`，程式將以互動式步驟引導您完成重要設定：資料庫存放路徑、用於盲區分析與報告生成的 LLM 供應商（金鑰自指定環境變數讀取，絕不會寫入檔案），以及用於混合搜尋的 Embedding 模型（支援立即下載模型）。系統會自動探測本機伺服器（Ollama、LM Studio）中已載入的模型，方便直接挑選。重複執行亦十分安全：現有設定值會成為預設值，不會清除任何既有內容。

所有可設定選項皆存放於 `~/.research/config.toml`：未設定的選項會以附帶預設值的註解形式呈現，設定檔本身即是說明文件。使用 `research init --no-onboard` 可略過互動提示（當 stdin 非終端機時亦會自動略過，確保自動化腳本順暢執行）。

## 更新方式

| 安裝方式 | 更新指令 |
|----------|----------|
| curl 安裝腳本 (macOS/Linux) | 重新執行上述安裝腳本 |
| PowerShell 安裝腳本 (Windows) | 重新執行上述安裝指令 |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

驗證目前安裝版本：

```bash
research --version
```

## 指令參考

| 指令 | 說明 |
|------|------|
| `research init` | 初始化研究工作區（終端機互動式引導：選擇供應商、環境變數名稱、Embedding 模型與下載；使用 `--no-onboard` 略過） |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | 從 arXiv、Semantic Scholar、OpenAlex、Europe PMC (PubMed) 或預印本來源 (bioRxiv, medRxiv 等) 擷取論文；可直接關聯至指定主題 |
| `research ingest [--source zotero] [query] [--topic <id>]` | 透過本機 API 從執行中的 Zotero 讀取論文（支援透過 `ZOTERO_BASE_URL` 覆寫預設端點，需啟用 "Allow other applications on this computer to communicate with Zotero"）；若無查詢條件則抓取整個文庫。不包含於 `--source all` |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | 擷取本機 PDF 檔案（儲存全文並支援全文檢索） |
| `research import <file\|dir>` | 匯入 BibTeX/BibLaTeX、CSL-JSON 或 Zotero JSON 檔案（桌面匯出或 API 結構），保留摘要、標籤與正規化 DOI；文庫中既有的論文將自動略過 |
| `research index [--rebuild]` | 建置或重新建置搜尋索引（FTS 全文檢索 + 向量索引） |
| `research reingest [--missing-pages]` | 重新擷取已儲存的 PDF 內文（為先前缺少頁碼標記的內容補上頁碼） |
| `research query <q> [--evidence]` | 檢索論文 — 向量可用時採用詞彙+語意混合搜尋；`--evidence` 可同時顯示相符的章節、頁碼與正文摘錄 |
| `research references <id> [--cited-by] [--intents]` | 從 OpenAlex 取得引文圖譜關聯邊；`--cited-by` 代表反向引文，`--intents` 加入 Semantic Scholar 引用意圖標籤 |
| `research gaps [--topic <id>]` | 分析知識覆蓋盲區（CLI 模式：若有設定則使用 `[llm]`） |
| `research report --topic <id>` | 產出文獻綜述報告（CLI 模式：若有設定則使用 `[llm]`） |
| `research topics list` | 列出所有研究主題 |
| `research topics add <name> [--parent <id>]` | 新增研究主題（使用 `--parent` 建立子主題） |
| `research read <id> [--status <status>] [--rating <1-5>]` | 更新閱讀狀態或評分 |
| `research read <id> --body` | 印出論文已儲存的內文 |
| `research status` | 檢視整體研究狀態總覽 |
| `research mcp` | 啟動 stdio MCP 伺服端（別名: `serve`） |

所有子指令皆支援全域 `--db <path>` 參數，可指定特定的資料庫檔案代替預設的 `~/.research/research.db` — 適合用於隔離環境或測試工作區。

## 系統需求

- Rust 1.92+（僅在由原始碼編譯時需要；預先編譯之二進位檔無需任何依賴）
- 資料存放於 `~/.research/`（包含 `research.db` 索引、`config.toml`、`embeddings.idx`）
- Linux 預編譯二進位檔針對 glibc ≥ 2.38 (Ubuntu 24.04+)；因內建之 ONNX Runtime 缺乏 musl 預先編譯版本，故不提供 musl 建置
- 選用：僅在 **從 CLI 手動執行** `research gaps` / `research report` 時需要 LLM — 於 `~/.research/config.toml` 中配置 `[llm]`（`provider`, `model`, `api_key_env`）；金鑰將於執行時自指定環境變數讀取。MCP 流程完全無需此項設定
- 選用：透過 `~/.research/config.toml` 之 `[search]` 調整混合搜尋 — `provider = "local"`（預設，內建 ONNX 模型）或 `"openai"`（需配置 `openai_api_key_env`）；`model` 用於挑選本機模型（預設為 `BGESmallENV15`，可在 CPU 上運行的 384 維輕量模型）。關於目前刻意尚未實作的功能，請參閱 [ROADMAP.md](../../../ROADMAP.md)

## 參與貢獻

請參閱 [CONTRIBUTING.md](../../../CONTRIBUTING.md) 與 [ROADMAP.md](../../../ROADMAP.md)。歡迎提交 PR。

## 授權條款

[APACHE-2](../../../LICENSE)。
