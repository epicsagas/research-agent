<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> このドキュメントは [README.md](../../../README.md) の翻訳版です。
> 英語の原文が公式ドキュメント（source of truth）であり、最新情報が含まれている場合があります。

<div align="center">

# research-agent

> AIエージェントのための長期リサーチメモリ — 論文のインデックス、知識ギャップの発見、サーベイ レポートの生成

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
  <b>日本語</b> |
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

## これは何か？

research-agent は **AIエージェント向けのリサーチサーバー** です。論文（arXiv、Semantic Scholar、ローカルPDF）をローカルの SQLite ライブラリにインデックスし、エージェントが MCP（Model Context Protocol）経由で直接駆動します。論文の収集、検索、読書進捗の管理、独自モデルによるカバレッジギャップの分析、サーベイ レポートの保存までを完結できます。APIキーは不要です。

同じバイナリがターミナルやスクリプト向けのスタンドアロン CLI としても動作します。

## Web UI

<img width="49%" src="../../../assets/overview.png" alt="dashboard-overview" />
<img width="49%" src="../../../assets/papers.png" alt="dashboard-overview" />

## ショーケース

**[The Ontology Lineage](https://epicsagas.github.io/ontology-explorer/en/)** — research-agentで425編を収集・ギャップ分析・整理し、1ページにまとめたオントロジーリサーチサイト(韓国語 · 英語): 系譜タイムライン、統合アーキテクチャ、5段階ラーニングルート、全文献ブラウザ。

## 主な機能

| | 機能 | メリット |
|--|------|----------|
| 🤖 | MCP サーバー | エージェントが stdio 経由で直接操作できる 18 個のツール — APIキー不要 |
| 🧠 | エージェントネイティブ分析 | ギャップ分析とレポート作成がエージェント内部で実行：ツールが構造化された状態を渡し、エージェントが推論し、結果が永続化される |
| 🔗 | 引用グラフ | OpenAlex からの論文間参照エッジ（順方向・逆方向）、オプションで Semantic Scholar の引用意図（citation intent）ラベル付き |
| 📚 | 論文インデックス | arXiv、Semantic Scholar、OpenAlex、Europe PMC（PubMed）、bioRxiv 系プレプリント、ローカル PDF、Zotero（起動中のインスタンスまたはエクスポートファイル） |
| 🔍 | ハイブリッド検索 | FTS5 語彙検索 + ローカル ONNX セマンティック検索、RRF 統合 — オフライン動作 |
| 📂 | トピックツリー | サブトピックによる階層的な研究整理 |
| 📖 | リーディングトラッカー | 読書キューの管理、進捗追跡、評価付け |
| ⚡ | 単一バイナリ | ランタイム不要、追加サーバー不要 — `research` 1 つで動作 |

## クイックスタート

お使いの AI エージェントにプラグインとしてインストールします（Rust ツールチェーンは不要）：

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

Hermes はルートの `plugin.yaml` と `__init__.py` の `register(ctx)` を読み込みます。MCP をサポートしていないため、エージェントは同梱スキルの CLI コマンドを介して `research` を実行します。あらかじめバイナリをインストールしてください（`brew install epicsagas/tap/research-agent` または curl インストーラ）。SessionStart の自動インストールフックは hermes には適用されません。skills_guard がインストールスキャンをブロックする場合は、hermes 設定で `plugins.scan_on_install: false` を設定してください。

プラグインはセッション開始時に `research` バイナリを自動インストールし、18 個の MCP ツールを公開します。エージェントは独自モデルを使用して、論文の取り込み、検索、分析、レポート作成を自律的に行えます。

インストール後、エージェントに次のように尋ねてみてください：

- "arXiv からグラフ ニューラル ネットワークの学習に関する最近の論文を取り込んで"
- "私の <topic> カバレッジに残っている知識のギャップは何？"
- "<topic> に関するサーベイ レポートを生成して"

これらの要求がツールレベルでどのように処理されるかを示します:

| こう依頼すると | エージェントはツールをこう組み合わせます |
|---------|------------------|
| トピックを最初から最後まで調査 | `init` → `topic_add` → `ingest`（arXiv/S2、トピックに紐付け） → `topic_brief` → エージェントが独自モデルでブリーフを分析 → `gaps_record` → 記録されたギャップを狙って再度 `ingest` → `report_material` → `report_save` |
| すでに集めた論文を探す | `query_papers` → `paper_body(id, query=...)` 該当箇所をセクション・ページ付きで引用 |
| 読んだものを追跡 | `update_read`（ステータス、1-5 評価）。カバレッジ概要は `state` |
| ダッシュボードを開く | Bash: `research dashboard` を実行し、http://127.0.0.1:7777 が起動したと伝える |

すべての操作は 1 つのローカル SQLite ライブラリ（デフォルト `~/.research/research.db`）に対して行われます。プロジェクト専用ライブラリが必要な場合は、ホストの MCP 設定でサーバーに別のファイルを指定します（`"args": ["mcp", "--db", "./project.research.db"]`。`--db` はグローバルフラグです）。あるいは、エージェントにスキル/CLI 呼び出しで `--db` を付けさせても構いません。

## エージェントによる利用方法

プラグインはセッション開始時に `research` バイナリを自動インストールし、**stdio MCP サーバー**（`research mcp`）を起動します。エージェントがツールを直接検出して呼び出すため、人間が CLI コマンドを入力する必要はありません。

**ツール一覧** (18個): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`。

分析は**エージェントネイティブ**です：`topic_brief` と `report_material` が構造化されたライブラリ状態（論文、読書進捗、記録されたギャップ、カバレッジ）を渡し、エージェントが独自モデルで推論を行い、`gaps_record` / `report_save` がその結果を永続化します。本文を検証する際、`paper_body` ツールに任意の `query` パラメータを渡すことで、全文の代わりにセクションとページ位置付きの証拠スニペットを取得でき、コンテキストを大幅に節約できます。MCP フロー全体で `[llm]` 設定や API キーは一切不要です。

素の JSON-RPC でサーバーのスモークテストを実行できます：

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

`mcp` cargo 機能は**デフォルトで有効**です。CLI のみでビルドする場合は `cargo build --no-default-features` を使用してください。

## スタンドアロン CLI（補助機能）

手動での操作を好む場合、同じバイナリを通常の CLI として使用できます。

```bash
# macOS / Linux — ビルド済みバイナリ、Rust 不要
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — ビルド済みバイナリ、Rust 不要
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### ダッシュボードの利用

同じデータベースを可視化した画面です。ターミナルで起動し、ブラウザで開いてください:

```bash
research dashboard        # 起動後 http://127.0.0.1:7777 にアクセス（ループバック限定）
```

| 画面 | 表示内容 |
|--------|---------------|
| Overview | ライブラリの規模、読書の深さ、パイプラインのファネル、トピック別カバレッジ |
| Papers | ライブラリのテーブル。行をクリックして詳細を表示、内蔵リーダーでページ位置付き本文や保存済み PDF を開く、読書ステータスと評価を編集 |
| Pipeline | 発見から精読まで、各論文がどの段階にいるか |
| History | これまでの出来事を新しい順に並べたフィード |
| Results | 知識ギャップと生成済みレポート |
| Config | `[llm]`、ワークスペースパス、ダッシュボードのバインド設定を編集 |

典型的な流れ: Overview でトピック カバレッジを確認し、Results で記録されたギャップを眺め、ターミナルに戻ってそのギャップを狙った次の取り込みを実行し、Papers で読書進捗を追跡します。

### 初回実行オンボーディング

ターミナルで `research init` を実行すると、重要な設定項目を対話形式で案内します：データベースの場所、ギャップ分析とレポート生成に使用する LLM プロバイダー（キー自体は指定した環境変数から読み取られ、ファイルには保存されません）、ハイブリッド検索用の埋め込みモデル（モデルの即時ダウンロードオプション付き）。ローカルサーバー（Ollama、LM Studio）でロード済みのモデルを検出し、実際に利用可能なモデルから選択できます。再実行も安全です：既存の値がデフォルトとなり、設定がリセットされることはありません。

設定可能なすべての項目は `~/.research/config.toml` に反映されます。未設定のオプションはデフォルト値とともにコメントアウトされ、ファイル自体がドキュメントとして機能します。`research init --no-onboard` はプロンプトをスキップします（stdin がターミナルでない場合も自動的にスキップされ、スクリプトがブロックされません）。

## アップデート方法

| インストール方法 | コマンド |
|------------------|----------|
| curl インストーラ (macOS/Linux) | 上記のインストールスクリプトを再実行 |
| PowerShell インストーラ (Windows) | 上記のインストールコマンドを再実行 |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

インストール済みバージョンの確認：

```bash
research --version
```

## コマンド一覧

| コマンド | 説明 |
|----------|------|
| `research init` | 研究ワークスペースの初期化（ターミナル対話型オンボーディング：プロバイダー、環境変数キー名、埋め込みモデル + ダウンロード；`--no-onboard` でスキップ） |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | arXiv、Semantic Scholar、OpenAlex、Europe PMC（PubMed）、プレプリントサーバー（bioRxiv、medRxiv 等）から論文を取り込む（トピックへ直接紐付け可能） |
| `research ingest [--source zotero] [query] [--topic <id>]` | ローカル API 経由で実行中の Zotero から論文を読み込む（`ZOTERO_BASE_URL` でエンドポイント上書き可能、"Allow other applications on this computer to communicate with Zotero" を有効にする必要があります）；クエリなしの場合はライブラリ全体を取得。`--source all` には含まれません |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | ローカル PDF ファイルを取り込む（本文テキスト全体が保存され、検索可能になります） |
| `research import <file\|dir>` | BibTeX/BibLaTeX、CSL-JSON、または Zotero JSON ファイル（デスクトップエクスポートまたは API 形式）を取り込む（要約、タグ、正規化された DOI を含む）；ライブラリに既存の論文はスキップされます |
| `research index [--rebuild]` | 検索インデックスの構築または再構築（FTS + ベクトルインデックス） |
| `research reingest [--missing-pages]` | 保存済み PDF 本文の再抽出（ページマーカーが存在する前に取り込まれた本文にマーカーを追加） |
| `research query <q> [--evidence]` | 論文を検索 — 埋め込みが利用可能な場合は語彙+セマンティックのハイブリッド検索；`--evidence` は一致した本文テキストをセクション・ページ番号とともに表示 |
| `research references <id> [--cited-by] [--intents]` | OpenAlex から引用グラフのエッジを取得；`--cited-by` は向きを反転、`--intents` は Semantic Scholar の引用意図ラベルを追加 |
| `research gaps [--topic <id>]` | 知識ギャップを分析（CLI：設定されている場合は `[llm]` を使用） |
| `research report --topic <id>` | サーベイ レポートを生成（CLI：設定されている場合は `[llm]` を使用） |
| `research topics list` | すべてのトピックを一覧表示 |
| `research topics add <name> [--parent <id>]` | 新しいトピックを追加（`--parent` を指定してサブトピックを作成） |
| `research read <id> [--status <status>] [--rating <1-5>]` | 読書ステータスまたは評価を更新 |
| `research read <id> --body` | 論文の保存された本文テキストを出力 |
| `research status` | 研究状態の概要を表示 |
| `research mcp` | stdio MCP サーバーを起動（エイリアス: `serve`） |

すべてのサブコマンドはグローバルフラグ `--db <path>` を受け入れ、`~/.research/research.db` の代わりに特定のデータベースを使用できます（分離された環境やテストワークスペースに便利です）。

### ワークフロー例

**調査ループ: 取り込み、ギャップの発見、ギャップを埋める**

```bash
research init                                          # 初回のみ: DB + LLM 設定
research topics add "Graph DB internals"               # トピック ID が出力されます
research ingest "latch-free graph database" --topic <TOPIC_ID> --limit 20
research gaps --topic <TOPIC_ID>                       # 調査で見えた抜け
# ギャップ分析が指摘した点を狙って次の取り込みを実行します:
research ingest "MVCC snapshot isolation graph store" --topic <TOPIC_ID>
research status                                        # トピック別の論文・カバレッジ・ギャップ
```

`research gaps` と `research report` はオンボーディングで設定した `[llm]` プロバイダーを使用します。未設定の場合は失敗ではなくプレースホルダを返します。

**すでに取り込んだものを探す**

```bash
research query "latch-free transaction" --evidence     # 本文まで検索、セクション・ページを表示
research read <PAPER_ID> --body                        # 保存済み本文の全文を出力
research read <PAPER_ID> --status completed --rating 5
```

**プロジェクトごとのライブラリ**

```bash
research --db ./project.research.db init
research --db ./project.research.db ingest "your topic" --topic <TOPIC_ID>
research dashboard --db ./project.research.db
```

## システム要件

- Rust 1.92+（ソースからビルドする場合のみ；ビルド済みバイナリの場合は不要）
- データ保存場所: `~/.research/` (`research.db` インデックス、`config.toml`、`embeddings.idx`)
- Linux バイナリは glibc ≥ 2.38（Ubuntu 24.04+）を対象としています。同梱の ONNX ランタイムに musl 用のビルド済みバイナリがないため、musl はビルドされません。
- オプション: **CLI からのみ** `research gaps` / `research report` に使用する LLM — `~/.research/config.toml` で `[llm]` を設定（`provider`、`model`、`api_key_env`）；キーは実行時に指定の環境変数から読み取られます。MCP フローでは一切不要です。
- オプション: `~/.research/config.toml` の `[search]` によるハイブリッド検索のチューニング — `provider = "local"`（デフォルト、同梱の ONNX モデル）または `"openai"`（`openai_api_key_env` 付き）；`model` でローカルモデルを選択（デフォルトは `BGESmallENV15`、CPU で動作する 384 次元の軽量モデル）。あえて実装していない機能については [ROADMAP.md](../../../ROADMAP.md) を参照してください。

## 貢献について

[CONTRIBUTING.md](../../../CONTRIBUTING.md) および [ROADMAP.md](../../../ROADMAP.md) をご覧ください。プルリクエスト（PR）を歓迎します。

## ライセンス

[APACHE-2](../../../LICENSE)。
