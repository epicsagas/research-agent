<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Dieses Dokument ist eine Übersetzung von [README.md](../../../README.md).
> Die englische Version ist die maßgebliche Quelle (source of truth) und möglicherweise aktueller.

<div align="center">

# research-agent

> Ihr langfristiges Forschungsgedächtnis — Paper indexiert, Wissenslücken erkannt, Berichte generiert

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
  <b>Deutsch</b> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## Was ist das?

research-agent ist ein **Forschungsserver für KI-Agenten**: Er indexiert wissenschaftliche Arbeiten (arXiv, Semantic Scholar, lokale PDFs) in einer lokalen SQLite-Bibliothek, und Ihr Agent steuert ihn direkt über MCP — beim Erfassen, Durchsuchen, Nachverfolgen des Lesefortschritts, Analysieren von Wissenslücken mit eigenem Modell und Verfassen von Übersichtsberichten. Kein API-Schlüssel erforderlich.

Die gleiche Binärdatei funktioniert auch als eigenständiges CLI für Terminals und Skripte.

## Web UI

<img width="49%" src="../../../assets/overview.png" alt="dashboard-overview" />
<img width="49%" src="../../../assets/papers.png" alt="dashboard-overview" />

## Funktionen

| | Funktion | Warum es wichtig ist |
|--|----------|----------------------|
| 🤖 | MCP-Server | 18 Werkzeuge, die Ihr Agent direkt über stdio steuert — kein API-Schlüssel erforderlich |
| 🧠 | Agenten-native Analyse | Lückenanalyse und Berichte laufen innerhalb Ihres Agenten: Werkzeuge liefern strukturierten Zustand, der Agent schlussfolgert, Ergebnisse werden gespeichert |
| 🔗 | Zitationsgraph | Referenzkanten zwischen Papern von OpenAlex, vorwärts und rückwärts, optional beschriftet mit Semantic-Scholar-Zitationsabsichten |
| 📚 | Paper-Indexierung | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), bioRxiv-Preprints, lokale PDFs und Zotero (laufende Instanz oder Exportdateien) |
| 🔍 | Hybride Suche | FTS5-Volltextsuche + lokale ONNX-Semantiksuche, mittels RRF zusammengeführt — funktioniert offline |
| 📂 | Themenbäume | Hierarchische Organisation von Forschungsthemen mit Unterthemen |
| 📖 | Lesetracker | Warteschlange, Lesefortschritt verfolgen und Gelesenes bewerten |
| ⚡ | Einzelne Binärdatei | Keine Runtime, kein externer Server — einfach nur `research` |

## Schnellstart

Als Plugin in Ihrem KI-Agenten installieren — keine Rust-Toolchain erforderlich:

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

Hermes lädt `plugin.yaml` im Hauptverzeichnis und `register(ctx)` in `__init__.py`. Da Hermes kein MCP unterstützt, steuert der Agent `research` über die CLI-Befehle des mitgelieferten Skills — installieren Sie zuerst die Binärdatei (`brew install epicsagas/tap/research-agent` oder per curl-Installationsskript); der SessionStart-Autoinstallations-Hook gilt nicht für Hermes. Falls skills_guard den Installationsscan blockiert, setzen Sie `plugins.scan_on_install: false` in der Hermes-Konfiguration.

Das Plugin installiert die `research`-Binärdatei beim Sitzungsstart automatisch und stellt 18 MCP-Tools bereit, sodass der Agent selbstständig Paper erfassen, durchsuchen, analysieren und Berichte verfassen kann.

Sobald installiert, können Sie Ihren Agenten beispielsweise Folgendes fragen:

- "Erfasse aktuelle Paper zum Training von Graph Neural Networks von arXiv"
- "Welche Wissenslücken bestehen noch bei meinem Thema <topic>?"
- "Erstelle einen Übersichtsbericht für <topic>"

So sehen diese Anfragen auf Tool-Ebene aus:

| Ihre Anfrage | Der Agent verkettet |
|---------|------------------|
| Ein Thema von Anfang bis Ende untersuchen | `init` → `topic_add` → `ingest` (arXiv/S2, mit dem Thema verknüpft) → `topic_brief` → der Agent wertet das Briefing mit seinem eigenen Modell aus → `gaps_record` → erneutes `ingest`, gezielt auf die erfassten Lücken → `report_material` → `report_save` |
| Etwas bereits Erfasstes wiederfinden | `query_papers` → `paper_body(id, query=...)` zitiert die passende Stelle mit Abschnitt und Seite |
| Lesefortschritt verfolgen | `update_read` (Status, Bewertung 1-5); `state` für die Abdeckungsübersicht |
| Das Dashboard öffnen | Bash: `research dashboard`, dann meldet es, dass http://127.0.0.1:7777 läuft |

Alles läuft gegen eine einzige lokale SQLite-Bibliothek (Standard `~/.research/research.db`). Für eine projektspezifische Bibliothek richten Sie den Server in der MCP-Konfiguration des Hosts einmalig auf eine andere Datei (`"args": ["mcp", "--db", "./project.research.db"]`, `--db` ist ein globales Flag) oder lassen den Agenten `--db` bei Skill-/CLI-Aufrufen mitgeben.

## Wie Ihr Agent es nutzt

Das Plugin installiert die `research`-Binärdatei beim Start automatisch und startet den **stdio-MCP-Server** (`research mcp`) — der Agent entdeckt und nutzt die Tools direkt, ohne manuelle CLI-Eingaben.

**Tools** (18): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read` ·
`papers_missing_keywords` · `enrich_paper`.

Die Analyse ist **agenten-nativ**: `topic_brief` und `report_material` übergeben den strukturierten Bibliothekszustand (Paper, Lesestatus, erfasste Lücken, Abdeckung), der Agent zieht Schlüsse mit seinem eigenen Modell, und `gaps_record` / `report_save` sichern die Erkenntnisse. Bei der Textprüfung akzeptiert `paper_body` einen optionalen `query`-Parameter, um verankerte Textbelege (mit Abschnitt und Seitenzahl) anstelle des gesamten Volltexts zurückzugeben, was wertvollen Kontext spart. Im gesamten MCP-Ablauf ist weder eine `[llm]`-Konfiguration noch ein API-Schlüssel nötig.

Einfacher Funktionstest des Servers über reines JSON-RPC:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

Das Cargo-Feature `mcp` ist **standardmäßig aktiviert**; bauen Sie die reine CLI-Version mit `cargo build --no-default-features`.

## Eigenständiges CLI (sekundär)

Bevorzugen Sie die manuelle Steuerung? Dieselbe Binärdatei funktioniert als vollwertiges CLI.

```bash
# macOS / Linux — vorgefertigte Binärdatei, kein Rust erforderlich
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — vorgefertigte Binärdatei, kein Rust erforderlich
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Das Dashboard verwenden

Dies ist die visuelle Seite derselben Datenbank. Im Terminal starten, im Browser ansehen:

```bash
research dashboard        # danach http://127.0.0.1:7777 öffnen (nur Loopback)
```

| Ansicht | Inhalt |
|--------|---------------|
| Overview | Bibliotheksgröße, Lesetiefe, Pipeline-Trichter, Themenabdeckung |
| Papers | Die Bibliothekstabelle; auf eine Zeile klicken für Details, im integrierten Leser seitenverankerte Volltexte oder gespeicherte PDFs öffnen, Lesestatus und Bewertung bearbeiten |
| Pipeline | Wo jedes Paper steht, von der Entdeckung bis zur vertieften Lektüre |
| History | Chronologisch rückwärts laufender Feed aller Ereignisse |
| Results | Wissenslücken und erstellte Berichte |
| Config | `[llm]`, Workspace-Pfad und Dashboard-Bindeeinstellungen bearbeiten |

Typischer Umlauf: Themenabdeckung in Overview prüfen, erfasste Lücken in Results ansehen, im Terminal die nächste Erfassung gezielt auf diese Lücken ausrichten, den Lesefortschritt in Papers verfolgen.

### Onboarding beim ersten Start

Führen Sie `research init` in einem Terminal aus. Ein interaktiver Assistent führt Sie durch alle wichtigen Einstellungen: Datenbankspeicherort, LLM-Anbieter für Lückenanalyse und Berichterstellung (der Schlüssel wird aus der von Ihnen angegebenen Umgebungsvariable gelesen, nie in der Datei gespeichert) und das Embedding-Modell für die Hybridsuche (inklusive direktem Download). Lokale Server (Ollama, LM Studio) werden auf bereits geladene Modelle geprüft, damit Sie aus vorhandenen Modellen auswählen können. Erneutes Ausführen ist sicher: bestehende Werte dienen als Standard und nichts wird überschrieben.

Alle konfigurierbaren Optionen befinden sich in `~/.research/config.toml`: Nicht gesetzte Optionen sind mit ihren Standardwerten auskommentiert, sodass die Datei sich selbst dokumentiert. `research init --no-onboard` überspringt die Abfragen (geschieht auch automatisch, wenn stdin kein Terminal ist).

## Aktualisierung

| Installationsmethode | Befehl |
|----------------------|--------|
| curl-Installationsskript (macOS/Linux) | Installationsskript oben erneut ausführen |
| PowerShell-Installationsskript (Windows) | Installationsbefehl oben erneut ausführen |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Installierte Version überprüfen:

```bash
research --version
```

## Befehle

| Befehl | Beschreibung |
|--------|--------------|
| `research init` | Forschungsworkspace initialisieren (interaktives Onboarding im Terminal: Anbieter, Umgebungsvariable für Schlüssel, Embedding-Modell + Download; `--no-onboard` zum Überspringen) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | Paper von arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) oder Preprint-Servern (bioRxiv, medRxiv, …) erfassen; optional direkt mit einem Thema verknüpfen |
| `research ingest [--source zotero] [query] [--topic <id>]` | Paper aus laufendem Zotero über lokale API einlesen (`ZOTERO_BASE_URL` überschreibt Standard-Endpunkt, "Allow other applications on this computer to communicate with Zotero" muss aktiviert sein); ohne Abfrage wird die gesamte Bibliothek geladen. Nicht Teil von `--source all` |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | Lokale PDF-Dateien erfassen (Volltext wird gespeichert und durchsuchbar gemacht) |
| `research import <file\|dir>` | BibTeX/BibLaTeX-, CSL-JSON- oder Zotero-JSON-Dateien importieren (mit Abstracts, Tags und normierten DOIs); bereits vorhandene Paper werden übersprungen |
| `research index [--rebuild]` | Suchindex aufbauen oder neu erstellen (FTS + Vektorindex) |
| `research reingest [--missing-pages]` | Gespeicherte PDF-Texte erneut extrahieren (fügt Seitenmarkierungen hinzu) |
| `research query <q> [--evidence]` | Paper durchsuchen — hybrid lexikalisch+semantisch, wenn Embeddings verfügbar; `--evidence` zeigt übereinstimmenden Text mit Abschnitt und Seite an |
| `research references <id> [--cited-by] [--intents]` | Zitationskanten von OpenAlex abrufen; `--cited-by` kehrt die Richtung um, `--intents` fügt Zitationsabsichten von Semantic Scholar hinzu |
| `research gaps [--topic <id>]` | Wissenslücken analysieren (CLI: verwendet `[llm]`, falls konfiguriert) |
| `research report --topic <id>` | Übersichtsbericht erstellen (CLI: verwendet `[llm]`, falls konfiguriert) |
| `research topics list` | Alle Themen auflisten |
| `research topics add <name> [--parent <id>]` | Neues Thema hinzufügen (`--parent` angeben, um ein Unterthema zu erstellen) |
| `research read <id> [--status <status>] [--rating <1-5>]` | Lesestatus oder Bewertung aktualisieren |
| `research read <id> --body` | Gespeicherten Volltext eines Papers ausgeben |
| `research status` | Gesamtstatus der Forschung anzeigen |
| `research mcp` | Den stdio-MCP-Server starten (Alias: `serve`) |

Jeder Unterbefehl unterstützt das globale Flag `--db <path>`, um eine bestimmte Datenbankdatei anstelle von `~/.research/research.db` zu verwenden.

### Beispiel-Workflows

**Forschungsschleife: erfassen, Lücken finden, Lücken schließen**

```bash
research init                                          # nur beim ersten Start: richtet DB + LLM ein
research topics add "Graph DB internals"               # gibt die Themen-ID aus
research ingest "latch-free graph database" --topic <TOPIC_ID> --limit 20
research gaps --topic <TOPIC_ID>                       # was die Recherche offengelegt hat
# die nächste Erfassung auf das richten, was die Lückenanalyse bemängelt hat:
research ingest "MVCC snapshot isolation graph store" --topic <TOPIC_ID>
research status                                        # Paper, Abdeckung und Lücken je Thema
```

`research gaps` und `research report` verwenden den beim Onboarding konfigurierten `[llm]`-Anbieter; ohne einen liefern sie einen Platzhalter zurück, statt zu scheitern.

**Etwas bereits Erfasstes wiederfinden**

```bash
research query "latch-free transaction" --evidence     # sucht bis in den Volltext, zeigt Abschnitt und Seite
research read <PAPER_ID> --body                        # gibt den gespeicherten Volltext aus
research read <PAPER_ID> --status completed --rating 5
```

**Eine Bibliothek pro Projekt**

```bash
research --db ./project.research.db init
research --db ./project.research.db ingest "your topic" --topic <TOPIC_ID>
research dashboard --db ./project.research.db
```

## Voraussetzungen

- Rust 1.92+ (nur erforderlich, wenn aus dem Quellcode kompiliert wird; vorgefertigte Binärdateien benötigen kein Rust)
- Daten werden unter `~/.research/` gespeichert (`research.db`, `config.toml`, `embeddings.idx`)
- Linux-Binärdateien erfordern glibc ≥ 2.38 (Ubuntu 24.04+); musl wird nicht unterstützt, da die integrierte ONNX-Runtime keine vorgefertigten musl-Bibliotheken anbietet
- Optional: ein LLM für `research gaps` / `research report` **ausschließlich für die CLI** — `[llm]` in `~/.research/config.toml` eintragen (`provider`, `model`, `api_key_env`); der Schlüssel wird zur Laufzeit aus der Umgebungsvariable gelesen. Der MCP-Workflow benötigt dies nicht
- Optional: Feineinstellung der Hybridsuche via `[search]` in `~/.research/config.toml` — `provider = "local"` (Standard, integriertes ONNX-Modell) oder `"openai"` mit `openai_api_key_env`; `model` wählt das lokale Modell (Standard `BGESmallENV15`, leichtgewichtiges 384-dim-Modell für CPU). Siehe [ROADMAP.md](../../../ROADMAP.md)

## Mitwirken

Siehe [CONTRIBUTING.md](../../../CONTRIBUTING.md) und [ROADMAP.md](../../../ROADMAP.md). Pull Requests sind herzlich willkommen.

## Lizenz

[APACHE-2](../../../LICENSE).
