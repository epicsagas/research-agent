<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> 本文档是 [README.md](../../../README.md) 的中文翻译版本。
> 英文原版为官方权威来源（source of truth），内容可能更新更及时。

<div align="center">

# research-agent

> AI Agent 的长期学术研究记忆库 — 索引论文、发掘知识盲区、生成综述报告

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
  <b>简体中文</b> |
  <a href="../zh-Hant/README.md">繁體中文</a> |
  <a href="../es/README.md">Español</a> |
  <a href="../fr/README.md">Français</a> |
  <a href="../de/README.md">Deutsch</a> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## 这是什么？

research-agent 是一款 **专为 AI Agent 打造的学术研究服务端**：它将论文（arXiv、Semantic Scholar、本地 PDF 等）索引到本地 SQLite 文献库中，您的 Agent 通过 MCP（Model Context Protocol）即可直接驱动 —— 执行论文采集、检索、跟踪阅读进度、借助 Agent 自带模型分析知识盲区（coverage gap），并生成整理综述报告。无需配置任何第三方 API 密钥。

同一个二进制可执行文件也可用作面向终端和自动化脚本的独立 CLI。

## Web UI

<img width="49%" src="../../../assets/overview.png" alt="dashboard-overview" />
<img width="49%" src="../../../assets/papers.png" alt="dashboard-overview" />

## 展示案例

**[The Ontology Lineage](https://epicsagas.github.io/ontology-explorer/en/)** — 使用 research-agent 收集、差距分析并整理的 425 篇文献，浓缩为一个静态页面（韩语 · 英语）：谱系时间线、集成架构、五阶段学习路线与全文浏览。

## 功能特性

| | 特性 | 价值 |
|--|------|------|
| 🤖 | MCP 服务端 | 18 个工具直接供 Agent 通过 stdio 调用 — 无需额外 API 密钥 |
| 🧠 | Agent 原生分析 | 知识盲区分析与报告生成在 Agent 内部完成：工具提供结构化状态，Agent 负责推理并持久化结果 |
| 🔗 | 引文图谱 | 基于 OpenAlex 获取论文间正向/反向引用边，支持 Semantic Scholar 引用意图（citation intent）标签 |
| 📚 | 论文索引 | 支持 arXiv、Semantic Scholar、OpenAlex、Europe PMC (PubMed)、bioRxiv 等预印本、本地 PDF 以及 Zotero（正在运行的客户端或导出文件） |
| 🔍 | 混合搜索 | FTS5 词汇搜索 + 本地 ONNX 语义向量搜索，RRF 融合重排 — 完全支持离线运行 |
| 📂 | 主题树结构 | 借助子主题分层组织研究脉络 |
| 📖 | 阅读跟踪器 | 排队管理、进度跟踪与文献评分 |
| ⚡ | 单一二进制 | 无额外运行时，无需常驻后台服务 — 仅需一个 `research` |

## 快速上手

作为插件安装到您的 AI Agent 中 — 无需 Rust 工具链：

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
grok plugin marketplace add epicsagas/research-agent
grok plugin install research-agent@research-agent --trust
```

### Hermes Agent

```bash
hermes plugins install https://github.com/epicsagas/research-agent
hermes plugins enable research-agent
```

Hermes 会加载根目录下的 `plugin.yaml` 以及 `__init__.py` 中的 `register(ctx)`。由于 Hermes 不支持 MCP，Agent 会通过内置 Skill 的 CLI 命令调用 `research` — 请先安装二进制文件（`brew install epicsagas/tap/research-agent` 或 curl 安装脚本）；SessionStart 自动安装 Hook 不适用于 Hermes。如果 skills_guard 拦截了安装检查，请在 Hermes 配置中设置 `plugins.scan_on_install: false`。

插件会在会话启动时自动安装 `research` 二进制文件并暴露 18 个 MCP 工具，Agent 便可以使用自身模型自主采集、检索、分析并生成报告。

安装完成后，您可以这样向 Agent 提问：

- "从 arXiv 采集关于图神经网络（GNN）训练的最新论文"
- "我当前对 <topic> 的调研覆盖中还存在哪些知识盲区？"
- "为 <topic> 生成一份文献综述报告"

这些请求在工具层面是这样完成的：

| 您这样提问 | Agent 会这样串联工具 |
|---------|------------------|
| 从头到尾调研一个主题 | `init` → `topic_add` → `ingest`（arXiv/S2，关联到主题） → `topic_brief` → Agent 用自身模型分析摘要 → `gaps_record` → 针对已记录的盲区再次 `ingest` → `report_material` → `report_save` |
| 查找已收录的论文 | `query_papers` → `paper_body(id, query=...)` 引用匹配的段落及其章节、页码 |
| 跟踪阅读进度 | `update_read`（状态、1-5 评分）；覆盖度概览用 `state` |
| 打开仪表盘 | Bash：运行 `research dashboard`，然后告诉您 http://127.0.0.1:7777 已启动 |

所有操作都针对同一个本地 SQLite 文献库（默认 `~/.research/research.db`）。如果需要项目专属的文献库，只需在宿主的 MCP 配置中让服务器指向另一个文件（`"args": ["mcp", "--db", "./project.research.db"]`，`--db` 是全局参数），或者让 Agent 在 Skill/CLI 调用时带上 `--db`。

## Agent 的使用方式

插件会在会话启动时自动安装 `research` 二进制文件并启动 **stdio MCP 服务端**（`research mcp`）—— Agent 会自动发现并直接调用这些工具，无需人工输入 CLI 命令。

**可用工具**（共 18 个）: `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`。

分析流程采用 **Agent 原生机制**：`topic_brief` 与 `report_material` 负责传递结构化的文献库状态（论文列表、阅读进度、已记录的盲区、覆盖度），Agent 借助自身模型进行推理，再通过 `gaps_record` / `report_save` 持久化保存结论。在审阅正文时，`paper_body` 工具可接收可选的 `query` 参数，返回带有章节与页码定位的证据片段，而不是全部正文，从而大幅节省上下文 Token。整个 MCP 交互流程无需任何 `[llm]` 配置或额外 API 密钥。

可以通过原生 JSON-RPC 对服务端进行冒烟测试：

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

`mcp` cargo 特性 **默认启用**；如需仅构建纯 CLI 版本，请运行 `cargo build --no-default-features`。

## 独立命令行（CLI）（辅助方式）

更喜欢手动在终端中操作？同一个二进制文件可以直接作为 CLI 使用。

```bash
# macOS / Linux — 预编译二进制，无需 Rust
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — 预编译二进制，无需 Rust
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### 使用仪表盘

这是同一个数据库的可视化界面。在终端中启动，然后在浏览器中打开：

```bash
research dashboard        # 启动后访问 http://127.0.0.1:7777（仅限本机回环）
```

| 界面 | 显示内容 |
|--------|---------------|
| Overview | 文献库规模、阅读深度、流水线漏斗、主题覆盖度 |
| Papers | 文献库表格；点击行查看详情，在内置阅读器中打开带页码定位的正文或已存 PDF，编辑阅读状态和评分 |
| Pipeline | 每篇论文从发现到精读所处的阶段 |
| History | 按时间倒序展示发生过的所有事件 |
| Results | 知识盲区和已生成的报告 |
| Config | 编辑 `[llm]`、工作区路径、仪表盘绑定设置 |

典型流程：先在 Overview 查看主题覆盖度，再到 Results 查看已记录的盲区，然后回到终端针对这些盲区执行下一轮采集，最后在 Papers 中跟踪阅读进度。

### 首次运行初始化

在终端中运行 `research init`，它将通过交互式向导引导您完成关键配置：数据库存储路径、用于盲区分析与报告生成的 LLM 提供商（API 密钥直接从您指定的环境变量中读取，绝不硬编码保存到配置文件中），以及用于混合搜索的 Embedding 嵌入模型（支持即时下载模型）。工具会自动探测本地服务（Ollama、LM Studio）中已加载的模型，方便直接选取现有模型。重复运行也是安全的：已有配置会自动作为默认值保留，不会清空数据。

所有可配置项均位于 `~/.research/config.toml` 中：未配置的选项会以带有默认值的注释形式展示，配置文件本身即是文档。使用 `research init --no-onboard` 可跳过交互式提示（当 stdin 为非终端时也会自动跳过，确保自动化脚本不被阻塞）。

## 更新方法

| 安装方式 | 更新命令 |
|----------|----------|
| curl 安装脚本 (macOS/Linux) | 重新执行上述安装脚本 |
| PowerShell 安装脚本 (Windows) | 重新执行上述安装命令 |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

验证当前安装的版本：

```bash
research --version
```

## 命令参考

| 命令 | 说明 |
|------|------|
| `research init` | 初始化研究工作区（终端交互式引导：选择模型提供商、环境变量名、Embedding 模型与下载；使用 `--no-onboard` 跳过） |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | 从 arXiv、Semantic Scholar、OpenAlex、Europe PMC (PubMed) 或预印本源 (bioRxiv, medRxiv 等) 采集论文；可直接关联到指定主题 |
| `research ingest [--source zotero] [query] [--topic <id>]` | 通过本地 API 从运行中的 Zotero 读取论文（支持通过 `ZOTERO_BASE_URL` 覆盖默认端点，需开启 "Allow other applications on this computer to communicate with Zotero"）；不带查询条件将拉取整个文库。不包含在 `--source all` 中 |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | 采集本地 PDF 文件（保存全文并支持全文检索） |
| `research import <file\|dir>` | 导入 BibTeX/BibLaTeX、CSL-JSON 或 Zotero JSON 文件（桌面导出或 API 结构），保留摘要、标签和规范化 DOI；已存在于文库中的论文会自动跳过 |
| `research index [--rebuild]` | 构建或重新构建搜索索引（FTS 全文检索 + 向量索引） |
| `research reingest [--missing-pages]` | 重新提取已存 PDF 正文（为早前未带页码标记的内容补充页码） |
| `research query <q> [--evidence]` | 检索论文 — 嵌入向量可用时采用词汇+语义混合搜索；`--evidence` 可一并展示匹配的章节、页码及正文片段 |
| `research references <id> [--cited-by] [--intents]` | 从 OpenAlex 获取引文图谱关联边；`--cited-by` 表示反向引文，`--intents` 添加 Semantic Scholar 引用意图标签 |
| `research gaps [--topic <id>]` | 分析知识覆盖盲区（CLI 模式：若已配置则使用 `[llm]`） |
| `research report --topic <id>` | 生成学术综述报告（CLI 模式：若已配置则使用 `[llm]`） |
| `research topics list` | 列出全部研究主题 |
| `research topics add <name> [--parent <id>]` | 新增研究主题（使用 `--parent` 创建子主题） |
| `research read <id> [--status <status>] [--rating <1-5>]` | 更新阅读状态或评分 |
| `research read <id> --body` | 打印已保存的论文正文 |
| `research status` | 查看整体研究状态概览 |
| `research mcp` | 启动 stdio MCP 服务端（别名: `serve`） |

所有子命令均支持全局 `--db <path>` 参数以指定特定的数据库文件，代替默认的 `~/.research/research.db` — 便于在隔离环境或测试工作区中使用。

### 典型工作流示例

**调研闭环：采集、发现盲区、填补盲区**

```bash
research init                                          # 仅首次运行：初始化数据库与 LLM 设置
research topics add "Graph DB internals"               # 会输出主题 ID
research ingest "latch-free graph database" --topic <TOPIC_ID> --limit 20
research gaps --topic <TOPIC_ID>                       # 调研中暴露的空白
# 针对盲区分析指出的问题执行下一轮采集：
research ingest "MVCC snapshot isolation graph store" --topic <TOPIC_ID>
research status                                        # 各主题的论文、覆盖度、盲区
```

`research gaps` 与 `research report` 使用初始化时配置的 `[llm]` 提供商；未配置时会返回占位结果而不是报错。

**查找已采集的内容**

```bash
research query "latch-free transaction" --evidence     # 连正文一起检索，显示章节和页码
research read <PAPER_ID> --body                        # 输出已保存的正文全文
research read <PAPER_ID> --status completed --rating 5
```

**每个项目一个独立文献库**

```bash
research --db ./project.research.db init
research --db ./project.research.db ingest "your topic" --topic <TOPIC_ID>
research dashboard --db ./project.research.db
```

## 运行要求

- Rust 1.92+（仅在从源码编译时需要；预编译二进制无需任何依赖）
- 数据存储于 `~/.research/` 目录（包含 `research.db` 索引库、`config.toml`、`embeddings.idx`）
- Linux 预编译二进制面向 glibc ≥ 2.38 (Ubuntu 24.04+)；由于内置 ONNX Runtime 缺少 musl 预编译库，暂不支持 musl
- 可选：仅在 **从 CLI 手动执行** `research gaps` / `research report` 时需要 LLM — 在 `~/.research/config.toml` 中配置 `[llm]`（`provider`, `model`, `api_key_env`）；密钥将在调用时从指定环境变量中读取。MCP 流程完全不需要此配置
- 可选：在 `~/.research/config.toml` 中通过 `[search]` 调整混合搜索 — `provider = "local"`（默认，内置 ONNX 模型）或 `"openai"`（需配置 `openai_api_key_env`）；`model` 用于选择本地模型（默认为 `BGESmallENV15`，可在 CPU 上运行的 384 维轻量模型）。关于尚未实现的功能，请参阅 [ROADMAP.md](../../../ROADMAP.md)

## 贡献指南

请参阅 [CONTRIBUTING.md](../../../CONTRIBUTING.md) 和 [ROADMAP.md](../../../ROADMAP.md)。欢迎提交 PR。

## 许可证

[APACHE-2](../../../LICENSE)。
