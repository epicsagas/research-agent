<!-- Translated from README.md @ commit e45d53a (2026-09-09) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Este documento é uma tradução de [README.md](../../../README.md).
> A versão em inglês é a fonte oficial e autoritativa (source of truth) e pode estar mais atualizada.

<div align="center">

# research-agent

> Memória de pesquisa de longo prazo para agentes de IA — artigos indexados, lacunas identificadas, relatórios gerados

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
  <a href="../zh-Hant/README.md">繁體中文</a> |
  <a href="../es/README.md">Español</a> |
  <a href="../fr/README.md">Français</a> |
  <a href="../de/README.md">Deutsch</a> |
  <b>Português</b> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## O que é isto?

O research-agent é um **servidor de pesquisa para agentes de IA**: ele indexa artigos científicos (arXiv, Semantic Scholar, PDFs locais) em uma biblioteca SQLite local, e o seu agente o comanda diretamente via MCP — ingerindo, pesquisando, monitorando o progresso da leitura, analisando lacunas de cobertura (coverage gaps) com o seu próprio modelo e arquivando relatórios de revisão. Nenhuma chave de API é necessária.

O mesmo executável binário também funciona como uma CLI independente para terminais e scripts.

## Recursos principais

| | Recurso | Por que é importante |
|--|---------|----------------------|
| 🤖 | Servidor MCP | 16 ferramentas que seu agente comanda diretamente via stdio — sem necessidade de chaves de API |
| 🧠 | Análise nativa do agente | A análise de lacunas e os relatórios são executados dentro do agente: as ferramentas entregam o estado estruturado, o agente raciocina e os resultados são persistidos |
| 🔗 | Grafo de citações | Arestas de referência entre artigos via OpenAlex, direta e reversa, opcionalmente rotuladas com intenções de citação do Semantic Scholar |
| 📚 | Indexação de artigos | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), preprints estilo bioRxiv, PDFs locais e Zotero (instância ativa ou arquivos exportados) |
| 🔍 | Busca híbrida | Busca lexical FTS5 + busca semântica ONNX local, unificadas por RRF — funciona totalmente offline |
| 📂 | Árvores de tópicos | Organização hierárquica das pesquisas por subtópicos |
| 📖 | Rastreador de leitura | Fila de leitura, acompanhamento de progresso e avaliação do que foi lido |
| ⚡ | Binário único | Sem dependências de runtime, sem servidores externos — apenas `research` |

## Início Rápido

Instale-o como plugin no seu agente de IA — não requer instalação da toolchain Rust:

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

O Hermes carrega o arquivo raiz `plugin.yaml` e a função `register(ctx)` em `__init__.py`. Como o Hermes não possui suporte a MCP, o agente controla o `research` por meio dos comandos CLI da skill incluída — instale primeiro o binário (`brew install epicsagas/tap/research-agent` ou pelo script de instalação curl); o hook de autoinstalação SessionStart não se aplica ao Hermes. Se o skills_guard bloquear a verificação de instalação, defina `plugins.scan_on_install: false` na configuração do Hermes.

O plugin instala automaticamente o binário `research` ao iniciar a sessão e expõe 16 ferramentas MCP, permitindo que o agente ingira, busque, analise com seu próprio modelo e elabore relatórios de forma autônoma.

Depois de instalado, experimente pedir coisas como:

- "Colete artigos recentes sobre treinamento de redes neurais em grafos do arXiv"
- "Quais lacunas de conhecimento ainda restam na minha cobertura de <topic>?"
- "Gere um relatório de levantamento para <topic>"

## Como o seu agente utiliza

O plugin instala automaticamente o binário `research` na inicialização da sessão e inicia o **servidor MCP stdio** (`research mcp`) — o agente descobre e aciona as ferramentas diretamente, sem a necessidade de digitação humana de comandos de terminal.

**Ferramentas** (16): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read`.

A análise é **nativa do agente**: `topic_brief` e `report_material` fornecem o estado estruturado da biblioteca (artigos, progresso de leitura, lacunas registradas, cobertura), o agente raciocina sobre ele utilizando seu próprio modelo e `gaps_record` / `report_save` persistem as descobertas. Nenhuma configuração `[llm]` ou chave de API é requerida em qualquer etapa do fluxo MCP.

Teste rápido de fumaça do servidor via JSON-RPC puro:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

A funcionalidade cargo `mcp` vem **habilitada por padrão**; para compilar apenas a CLI, utilize `cargo build --no-default-features`.

## CLI independente (secundária)

Prefere operar manualmente? O mesmo binário funciona perfeitamente como uma CLI tradicional.

```bash
# macOS / Linux — binário pré-compilado, sem necessidade de Rust
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — binário pré-compilado, sem necessidade de Rust
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Configuração no primeiro uso

Execute `research init` no terminal e ele apresentará um assistente interativo com as configurações fundamentais: localização do banco de dados, provedor de LLM para análise de lacunas e relatórios (a chave é lida da variável de ambiente indicada, nunca gravada no arquivo) e o modelo de embedding para a busca híbrida (com opção de download imediato). Servidores locais (Ollama, LM Studio) são consultados quanto a modelos carregados para que você selecione entre o que realmente existe. Executá-lo novamente é seguro: valores existentes viram os padrões e nada é apagado.

Tudo o que for configurável é registrado em `~/.research/config.toml`: opções ainda não definidas aparecem comentadas com seus valores padrão, servindo o próprio arquivo como documentação. `research init --no-onboard` pula as perguntas interativas (o que também acontece automaticamente se o stdin não for um terminal).

## Atualização

| Método | Comando |
|--------|---------|
| Instalador curl (macOS/Linux) | Execute novamente o script de instalação acima |
| Instalador PowerShell (Windows) | Execute novamente o comando de instalação acima |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Verificar versão instalada:

```bash
research --version
```

## Comandos

| Comando | Descrição |
|---------|-----------|
| `research init` | Inicializa o espaço de trabalho (assistente interativo no terminal: provedor, variável de ambiente da chave, modelo de embedding + download; `--no-onboard` para pular) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all]` | Ingere artigos do arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) ou servidores de preprints (bioRxiv, medRxiv, …) |
| `research ingest [--source zotero] [query]` | Lê artigos do Zotero em execução pela API local (requer ativar "Allow other applications on this computer to communicate with Zotero"); sem consulta, puxa a biblioteca inteira. Não incluído em `--source all` |
| `research ingest --source pdf --path <file\|dir>` | Ingere arquivos PDF locais (o texto completo é armazenado e pesquisável) |
| `research import <file\|dir>` | Importa arquivos BibTeX/BibLaTeX, CSL-JSON ou Zotero JSON (exportação local ou formato de API), com resumos, tags e DOIs normalizados; artigos já existentes são ignorados |
| `research index [--rebuild]` | Cria ou reconstrói o índice de busca (FTS + índice vetorial) |
| `research reingest [--missing-pages]` | Reextrai o texto de PDFs armazenados (adiciona marcadores de página a conteúdos anteriores) |
| `research query <q> [--evidence]` | Pesquisa artigos — híbrida lexical+semântica quando embeddings disponíveis; `--evidence` exibe o texto correspondente com seção e página |
| `research references <id> [--cited-by] [--intents]` | Obtém arestas do grafo de citações do OpenAlex; `--cited-by` inverte o sentido, `--intents` rotula com intenções de citação do Semantic Scholar |
| `research gaps [--topic <id>]` | Analisa lacunas de conhecimento (CLI: usa `[llm]` se configurado) |
| `research report --topic <id>` | Gera relatório de levantamento (CLI: usa `[llm]` se configurado) |
| `research topics list` | Lista todos os tópicos de pesquisa |
| `research topics add <name>` | Adiciona um novo tópico |
| `research read <id> [--status <status>] [--rating <1-5>]` | Atualiza status de leitura ou avaliação |
| `research read <id> --body` | Imprime o texto integral armazenado de um artigo |
| `research status` | Exibe o panorama geral da pesquisa |
| `research mcp` | Inicia o servidor MCP stdio (alias: `serve`) |

Todos os subcomandos aceitam a flag global `--db <path>` para apontar para um banco de dados específico em vez de `~/.research/research.db` — ideal para ambientes de teste e espaços de trabalho isolados.

## Requisitos

- Rust 1.92+ (necessário apenas se for compilar a partir do código-fonte; binários pré-compilados não exigem nada)
- Os dados residem em `~/.research/` (`research.db`, `config.toml`, `embeddings.idx`)
- Binários Linux têm como alvo glibc ≥ 2.38 (Ubuntu 24.04+); musl não é suportado devido à falta de binários pré-compilados para a runtime ONNX integrada
- Opcional: um LLM para `research gaps` / `research report` **apenas ao usar a CLI** — configure `[llm]` em `~/.research/config.toml` (`provider`, `model`, `api_key_env`); a chave é lida da variável de ambiente no momento da execução. O fluxo MCP não precisa disso
- Opcional: ajuste da busca híbrida via `[search]` em `~/.research/config.toml` — `provider = "local"` (padrão, modelo ONNX embutido) ou `"openai"` com `openai_api_key_env`; `model` define o modelo local (padrão `BGESmallENV15`, modelo leve de 384 dimensões executado em CPU). Consulte [ROADMAP.md](../../../ROADMAP.md)

## Como contribuir

Consulte [CONTRIBUTING.md](../../../CONTRIBUTING.md) e [ROADMAP.md](../../../ROADMAP.md). Pull Requests são bem-vindos.

## Licença

[APACHE-2](../../../LICENSE).
