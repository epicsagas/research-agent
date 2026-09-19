<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Questo documento è una traduzione di [README.md](../../../README.md).
> La versione inglese costituisce la fonte autorevole di riferimento (source of truth) e potrebbe essere più aggiornata.

<div align="center">

# research-agent

> Memoria di ricerca a lungo termine per agenti IA — articoli indicizzati, lacune individuate, report generati

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
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <b>Italiano</b>
</p>

</div>

## Che cos'è?

research-agent è un **server di ricerca per agenti IA**: indicizza articoli scientifici (arXiv, Semantic Scholar, PDF locali) in una libreria SQLite locale e il vostro agente lo comanda direttamente via MCP — acquisizione, ricerca, monitoraggio della lettura, analisi delle lacune di copertura con il proprio modello e archiviazione di report di revisione. Non richiede alcuna chiave API.

Lo stesso file binario funziona anche come CLI autonoma per terminali e script.

## Web UI

<img width="49%" src="../../../assets/overview.png" alt="dashboard-overview" />
<img width="49%" src="../../../assets/papers.png" alt="dashboard-overview" />

## Vetrina

**[The Ontology Lineage](https://epicsagas.github.io/ontology-explorer/)** — 2.300 anni di ricerca ontologica in una pagina statica (coreano · inglese): cronologia, architettura integrata, percorso di apprendimento in cinque tappe ed esploratore dei 425 articoli — raccolti, analizzati e organizzati con research-agent.

## Funzionalità principali

| | Funzionalità | Perché è importante |
|--|--------------|---------------------|
| 🤖 | Server MCP | 18 strumenti che il vostro agente controlla direttamente via stdio — senza bisogno di chiavi API |
| 🧠 | Analisi nativa per agenti | L'analisi delle lacune e i report vengono elaborati all'interno dell'agente: gli strumenti forniscono lo stato strutturato, l'agente ragiona e i risultati vengono salvati |
| 🔗 | Grafo delle citazioni | Relazioni di riferimento tra articoli provenienti da OpenAlex, dirette e inverse, con etichettatura opzionale degli intenti di citazione da Semantic Scholar |
| 📚 | Indicizzazione degli articoli | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), preprint bioRxiv, PDF locali e Zotero (istanza in esecuzione o file esportati) |
| 🔍 | Ricerca ibrida | Ricerca lessicale FTS5 + semantica ONNX locale, combinate tramite RRF — funziona completamente offline |
| 📂 | Alberi tematici | Organizzazione gerarchica delle ricerche con sotto-argomenti |
| 📖 | Monitoraggio delle letture | Coda di lettura, tracciamento dello stato di avanzamento e valutazione degli articoli |
| ⚡ | Binario unico | Nessun runtime aggiuntivo, nessun server esterno — solo `research` |

## Avvio rapido

Installatelo come plugin nel vostro agente IA — non è richiesta la toolchain Rust:

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

Hermes carica il file `plugin.yaml` principale e `register(ctx)` in `__init__.py`. Poiché Hermes non supporta MCP, l'agente esegue `research` tramite i comandi CLI inclusi nello skill — installate prima il binario (`brew install epicsagas/tap/research-agent` o tramite script di installazione curl); l'hook SessionStart per l'installazione automatica non si applica a Hermes. Se skills_guard blocca la scansione dell'installazione, impostate `plugins.scan_on_install: false` nella configurazione di Hermes.

Il plugin installa automaticamente il binario `research` all'avvio della sessione ed espone 18 strumenti MCP, consentendo all'agente di raccogliere, cercare, analizzare con il proprio modello e redigere report in completa autonomia.

Una volta installato, potete chiedere all'agente cose come:

- "Raccogli da arXiv gli articoli recenti sull'addestramento delle Graph Neural Networks"
- "Quali lacune di conoscenza rimangono nella mia copertura di <topic>?"
- "Genera un report di revisione per <topic>"

Ecco come queste richieste si traducono a livello di strumenti:

| Voi chiedete | L'agente concatena |
|---------|------------------|
| Esplorare un argomento dall'inizio alla fine | `init` → `topic_add` → `ingest` (arXiv/S2, collegato all'argomento) → `topic_brief` → l'agente ragiona sul quadro con il proprio modello → `gaps_record` → un nuovo `ingest` mirato sulle lacune registrate → `report_material` → `report_save` |
| Trovare qualcosa di già raccolto | `query_papers` → `paper_body(id, query=...)` cita il passaggio corrispondente con sezione e pagina |
| Tenere traccia delle letture | `update_read` (stato, valutazione 1-5); `state` per la panoramica della copertura |
| Aprire la dashboard | Bash: `research dashboard`, poi vi segnala che http://127.0.0.1:7777 è attivo |

Tutto opera su un'unica libreria SQLite locale (predefinita `~/.research/research.db`). Per una libreria dedicata a un progetto, puntate il server su un file diverso nella configurazione MCP dell'host (`"args": ["mcp", "--db", "./project.research.db"]`, `--db` è un flag globale), oppure fate passare `--db` all'agente nelle invocazioni skill/CLI.

## Come lo usa il vostro agente

Il plugin installa automaticamente il binario `research` all'inizio della sessione e avvia il **server MCP stdio** (`research mcp`) — l'agente rileva e invoca direttamente gli strumenti, senza necessità di digitare comandi da terminale.

**Strumenti** (18): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`.

L'analisi è **nativa per l'agente**: `topic_brief` e `report_material` forniscono lo stato strutturato della libreria (articoli, stato di lettura, lacune registrate, copertura), l'agente elabora le considerazioni con il proprio modello e `gaps_record` / `report_save` persistono i risultati. Quando si esamina il testo, `paper_body` accetta un parametro facoltativo `query` per restituire frammenti di prova localizzati (con sezione e pagina) anziché l'intero corpo del testo, risparmiando contesto. Non è richiesta alcuna configurazione `[llm]` o chiave API durante l'uso tramite MCP.

Verifica rapida del server tramite JSON-RPC non formattato:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

La funzionalità cargo `mcp` è **abilitata per impostazione predefinita**; per compilare solo la versione CLI, utilizzate `cargo build --no-default-features`.

## CLI autonoma (secondaria)

Preferite gestirlo manualmente da terminale? Lo stesso binario funziona come una comune CLI.

```bash
# macOS / Linux — binario precompilato, Rust non richiesto
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — binario precompilado, Rust non richiesto
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Uso della dashboard

È il lato visivo dello stesso database. Avviatelo da terminale e consultatelo dal browser:

```bash
research dashboard        # poi aprite http://127.0.0.1:7777 (solo in loopback)
```

| Schermata | Cosa mostra |
|--------|---------------|
| Overview | Dimensione della libreria, profondità di lettura, funnel della pipeline, copertura per argomento |
| Papers | La tabella della libreria; cliccate su una riga per i dettagli, aprite il lettore integrato con testi ancorati alla pagina o il PDF salvato, modificate stato di lettura e valutazione |
| Pipeline | A che punto si trova ogni articolo, dalla scoperta alla lettura approfondita |
| History | Flusso cronologico inverso di tutto ciò che è successo |
| Results | Lacune di conoscenza e report generati |
| Config | Modifica di `[llm]`, percorso del workspace e impostazioni di binding della dashboard |

Ciclo tipico: controllate la copertura degli argomenti in Overview, esaminate le lacune registrate in Results, tornate al terminale per puntare la prossima raccolta su quelle lacune, poi seguite l'avanzamento della lettura in Papers.

### Configurazione guidata al primo avvio

Eseguite `research init` in un terminale e verrete guidati attraverso le impostazioni fondamentali: posizione del database, fornitore LLM per l'analisi delle lacune e i report (la chiave viene letta dalla variabile d'ambiente indicata, mai memorizzata nel file) e modello di embedding per la ricerca ibrida (con opzione di download immediato). I server locali (Ollama, LM Studio) vengono scansionati alla ricerca di modelli caricati per consentirvi di scegliere tra quelli effettivamente disponibili. L'esecuzione ripetuta è sicura: i valori attuali diventano i predefiniti e nulla viene azzerato.

Tutto ciò che è configurabile si trova in `~/.research/config.toml`: le opzioni non impostate appaiono commentate con i valori di default, fungendo da documentazione diretta. `research init --no-onboard` salta i prompt interattivi (ciò avviene anche in automatico quando stdin non è un terminale).

## Aggiornamento

| Metodo | Comando |
|--------|---------|
| Script di installazione curl (macOS/Linux) | Eseguite nuovamente lo script di installazione sopra indicato |
| Script PowerShell (Windows) | Eseguite nuovamente il comando di installazione sopra indicato |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Verificare la versione installata:

```bash
research --version
```

## Elenco comandi

| Comando | Descrizione |
|---------|-------------|
| `research init` | Inizializza l'ambiente di ricerca (configurazione interattiva da terminale: provider, variabile d'ambiente della chiave, modello di embedding + download; `--no-onboard` per saltare) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | Raccoglie articoli da arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) o server di preprint (bioRxiv, medRxiv, …); collegabile opzionalmente a un argomento |
| `research ingest [--source zotero] [query] [--topic <id>]` | Legge articoli da un'istanza Zotero attiva tramite API locale (`ZOTERO_BASE_URL` sovrascrive l'endpoint predefinito, richiede l'attivazione di "Consenti ad altre applicazioni di comunicare con Zotero"); senza query estrae l'intera libreria. Escluso da `--source all` |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | Raccoglie file PDF locali (il testo completo viene salvato e reso ricercabile) |
| `research import <file\|dir>` | Importa file BibTeX/BibLaTeX, CSL-JSON o Zotero JSON (esportazione desktop o formato API) con abstract, tag e DOI normalizzati; gli articoli già presenti vengono ignorati |
| `research index [--rebuild]` | Crea o ricostruisce l'indice di ricerca (FTS + indice vettoriale) |
| `research reingest [--missing-pages]` | Riestrae il testo dei PDF memorizzati (aggiunge i marcatori di pagina ai file acquisiti in precedenza) |
| `research query <q> [--evidence]` | Cerca articoli — ibrida lessicale+semantica quando gli embedding sono disponibili; `--evidence` mostra i passaggi testuali corrispondenti con sezione e pagina |
| `research references <id> [--cited-by] [--intents]` | Recupera le relazioni del grafo delle citazioni da OpenAlex; `--cited-by` inverte la direzione, `--intents` assegna le etichette di intento citazionale di Semantic Scholar |
| `research gaps [--topic <id>]` | Analizza le lacune di conoscenza (CLI: usa `[llm]` se configurato) |
| `research report --topic <id>` | Genera un report di revisione (CLI: usa `[llm]` se configurato) |
| `research topics list` | Elenca tutti gli argomenti |
| `research topics add <name> [--parent <id>]` | Aggiunge un nuovo argomento (specificare `--parent` per creare un sotto-argomento) |
| `research read <id> [--status <status>] [--rating <1-5>]` | Aggiorna lo stato di lettura o la valutazione |
| `research read <id> --body` | Stampa il corpo del testo archiviato di un articolo |
| `research status` | Mostra una panoramica dello stato della ricerca |
| `research mcp` | Avvia il server MCP stdio (alias: `serve`) |

Tutti i sottocomandi supportano il flag globale `--db <path>` per usare un database specifico al posto di `~/.research/research.db` — ideale per ambienti di test o isolati.

### Esempi di flussi di lavoro

**Ciclo di ricerca: raccogliere, individuare le lacune, colmarle**

```bash
research init                                          # solo al primo avvio: configura DB + LLM
research topics add "Graph DB internals"               # stampa l'ID dell'argomento
research ingest "latch-free graph database" --topic <TOPIC_ID> --limit 20
research gaps --topic <TOPIC_ID>                       # ciò che l'esplorazione ha lasciato intravedere
# puntate la raccolta successiva su ciò che l'analisi delle lacune ha segnalato:
research ingest "MVCC snapshot isolation graph store" --topic <TOPIC_ID>
research status                                        # articoli, copertura e lacune per argomento
```

`research gaps` e `research report` usano il provider `[llm]` configurato alla prima configurazione; senza, restituiscono un segnaposto invece di fallire.

**Trovare qualcosa di già raccolto**

```bash
research query "latch-free transaction" --evidence     # cerca fino nel corpo del testo, mostra sezione e pagina
research read <PAPER_ID> --body                        # stampa il testo completo archiviato
research read <PAPER_ID> --status completed --rating 5
```

**Una libreria per progetto**

```bash
research --db ./project.research.db init
research --db ./project.research.db ingest "your topic" --topic <TOPIC_ID>
research dashboard --db ./project.research.db
```

## Requisiti

- Rust 1.92+ (necessario solo se si compila dai sorgenti; i binari precompilati non richiedono nulla)
- I dati risiedono in `~/.research/` (`research.db`, `config.toml`, `embeddings.idx`)
- I binari Linux sono compilati per glibc ≥ 2.38 (Ubuntu 24.04+); musl non è supportato a causa dell'assenza di binari precompilati per il runtime ONNX integrato
- Opzionale: un LLM per `research gaps` / `research report` **esclusivamente da CLI** — impostate `[llm]` in `~/.research/config.toml` (`provider`, `model`, `api_key_env`); la chiave viene letta dalla variabile d'ambiente specificata al momento della chiamata. Il flusso MCP non ne ha mai bisogno
- Opzionale: personalizzazione della ricerca ibrida tramite `[search]` in `~/.research/config.toml` — `provider = "local"` (predefinito, modello ONNX integrato) oppure `"openai"` con `openai_api_key_env`; `model` seleziona il modello locale (predefinito `BGESmallENV15`, modello compatto a 384 dimensioni che gira su CPU). Consultate [ROADMAP.md](../../../ROADMAP.md)

## Contribuire

Consultate [CONTRIBUTING.md](../../../CONTRIBUTING.md) e [ROADMAP.md](../../../ROADMAP.md). Le pull request sono benvenute.

## Licenza

[APACHE-2](../../../LICENSE).
