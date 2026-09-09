<!-- Translated from README.md @ commit e45d53a (2026-09-09) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Ce document est une traduction de [README.md](../../../README.md).
> La version anglaise constitue la source de référence faisant foi (source of truth) et peut être plus à jour.

<div align="center">

# research-agent

> Mémoire de recherche à long terme pour agents IA — articles indexés, lacunes identifiées, rapports générés

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
  <b>Français</b> |
  <a href="../de/README.md">Deutsch</a> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## De quoi s'agit-il ?

research-agent est un **serveur de recherche pour agents IA** : il indexe des articles scientifiques (arXiv, Semantic Scholar, PDF locaux) dans une bibliothèque SQLite locale, et votre agent le pilote directement via MCP — ingestion, recherche, suivi de lecture, analyse des lacunes de couverture (coverage gaps) avec son propre modèle et archivage de rapports de synthèse. Aucune clé d'API n'est requise.

Le même binaire fonctionne également comme un CLI autonome pour le terminal et les scripts.

## Fonctionnalités

| | Fonctionnalité | Pourquoi c'est important |
|--|----------------|-------------------------|
| 🤖 | Serveur MCP | 16 outils que votre agent pilote directement via stdio — sans clé d'API |
| 🧠 | Analyse native à l'agent | L'analyse des lacunes et les rapports s'exécutent au sein de votre agent : les outils transmettent l'état structuré, l'agent raisonne et les résultats sont persistés |
| 🔗 | Graphe de citations | Liens de référence d'article à article issus d'OpenAlex, directs et inversés, avec étiquetage optionnel des intentions de citation Semantic Scholar |
| 📚 | Indexation d'articles | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), prépublications bioRxiv, PDF locaux et Zotero (instance active ou fichiers d'export) |
| 🔍 | Recherche hybride | Recherche lexicale FTS5 + sémantique ONNX locale, fusionnées par RRF — fonctionne entièrement hors ligne |
| 📂 | Arborescences de thèmes | Organisation hiérarchique des recherches à l'aide de sous-thèmes |
| 📖 | Suivi de lecture | Gestion de la file d'attente, suivi de la progression et notation des lectures |
| ⚡ | Binaire unique | Aucun runtime ni serveur externe — uniquement `research` |

## Démarrage rapide

Installez-le en tant que plugin dans votre agent IA — aucune chaîne d'outils Rust n'est requise :

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

Hermes charge le fichier racine `plugin.yaml` et `register(ctx)` dans `__init__.py`. Ne prenant pas en charge MCP, l'agent pilote `research` par les commandes CLI du skill inclus — installez d'abord le binaire (`brew install epicsagas/tap/research-agent` ou le script d'installation curl) ; le hook d'auto-installation SessionStart ne s'applique pas à Hermes. Si skills_guard bloque le scan d'installation, configurez `plugins.scan_on_install: false` dans la configuration d'Hermes.

Le plugin installe automatiquement le binaire `research` au démarrage de la session et expose 16 outils MCP, permettant à l'agent d'ingérer, chercher, analyser avec son propre modèle et consigner des rapports de manière autonome.

Une fois installé, demandez par exemple à votre agent :

- "Récupère les publications récentes sur l'entraînement des réseaux de neurones sur graphes depuis arXiv"
- "Quelles lacunes de connaissances subsistent dans ma couverture de <topic> ?"
- "Génère un rapport de synthèse pour <topic>"

## Comment votre agent l'utilise

Le plugin installe automatiquement le binaire `research` au démarrage de la session et lance le **serveur MCP stdio** (`research mcp`) — l'agent découvre et appelle directement les outils, sans intervention humaine en ligne de commande.

**Outils** (16) : `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read`.

L'analyse est **native à l'agent** : `topic_brief` et `report_material` transmettent l'état structuré de la bibliothèque (articles, avancement de lecture, lacunes enregistrées, couverture), l'agent effectue le raisonnement avec son propre modèle, et `gaps_record` / `report_save` sauvegardent les conclusions. Aucune configuration `[llm]` ni clé d'API n'est requise dans le flux MCP.

Testez rapidement le serveur avec du JSON-RPC brut :

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

La fonctionnalité cargo `mcp` est **activée par défaut** ; pour compiler uniquement le CLI, utilisez `cargo build --no-default-features`.

## CLI autonome (secondaire)

Vous préférez l'utiliser manuellement ? Le même binaire s'utilise comme un CLI ordinaire.

```bash
# macOS / Linux — binaire précompilé, sans Rust requis
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — binaire précompilé, sans Rust requis
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Premier lancement et configuration

Exécutez `research init` dans un terminal pour configurer les paramètres essentiels via un assistant interactif : emplacement de la base de données, fournisseur LLM pour l'analyse des lacunes et la génération de rapports (la clé est lue depuis la variable d'environnement désignée, jamais stockée dans le fichier), et modèle d'embedding pour la recherche hybride (avec option de téléchargement immédiat). Les serveurs locaux (Ollama, LM Studio) sont interrogés afin de proposer les modèles réellement disponibles. L'exécuter à nouveau est sans danger : les valeurs existantes servent de valeurs par défaut et rien n'est réinitialisé.

Tous les éléments configurables figurent dans `~/.research/config.toml` : les options non définies apparaissent en commentaire avec leurs valeurs par défaut. `research init --no-onboard` ignore les invites interactives (action également automatique lorsque stdin n'est pas un terminal).

## Mise à jour

| Méthode | Commande |
|---------|----------|
| Script d'installation curl (macOS/Linux) | Réexécutez le script d'installation ci-dessus |
| Script PowerShell (Windows) | Réexécutez la commande d'installation ci-dessus |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Vérification de la version installée :

```bash
research --version
```

## Commandes

| Commande | Description |
|----------|-------------|
| `research init` | Initialise l'espace de travail (assistant interactif en terminal : fournisseur, variable d'environnement de la clé, modèle d'embedding + téléchargement ; `--no-onboard` pour ignorer) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all]` | Ingère des articles depuis arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) ou des serveurs de prépublications (bioRxiv, medRxiv, …) |
| `research ingest [--source zotero] [query]` | Lit les articles depuis une instance Zotero en cours d'exécution via son API locale (requiert l'activation de "Autoriser d'autres applications à communiquer avec Zotero") ; sans requête, toute la bibliothèque est récupérée. Exclu de `--source all` |
| `research ingest --source pdf --path <file\|dir>` | Ingère des fichiers PDF locaux (le corps de texte complet est indexé et recherchable) |
| `research import <file\|dir>` | Importe des fichiers BibTeX/BibLaTeX, CSL-JSON ou Zotero JSON (export de bureau ou format API) avec résumés, étiquettes et DOI normalisés ; ignore les articles déjà présents |
| `research index [--rebuild]` | Crée ou reconstruit l'index de recherche (FTS + index vectoriel) |
| `research reingest [--missing-pages]` | Réextrait le corps des PDF stockés (ajoute les marqueurs de page aux articles ingérés avant leur support) |
| `research query <q> [--evidence]` | Recherche des articles — hybride lexical+sémantique lorsque les embeddings sont disponibles ; `--evidence` affiche l'extrait du texte correspondant avec section et page |
| `research references <id> [--cited-by] [--intents]` | Récupère les liens de citation depuis OpenAlex ; `--cited-by` inverse la direction, `--intents` ajoute les étiquettes d'intention de citation de Semantic Scholar |
| `research gaps [--topic <id>]` | Analyse les lacunes de connaissances (CLI : utilise `[llm]` si configuré) |
| `research report --topic <id>` | Génère un rapport de synthèse (CLI : utilise `[llm]` si configuré) |
| `research topics list` | Liste tous les thèmes de recherche |
| `research topics add <name>` | Ajoute un nouveau thème |
| `research read <id> [--status <status>] [--rating <1-5>]` | Met à jour le statut de lecture ou la note |
| `research read <id> --body` | Affiche le corps de texte stocké d'un article |
| `research status` | Affiche l'état global de la recherche |
| `research mcp` | Démarre le serveur MCP stdio (alias : `serve`) |

Toutes les sous-commandes acceptent l'option globale `--db <path>` pour désigner une base de données spécifique à la place de `~/.research/research.db` — très pratique pour les environnements de test ou isolés.

## Prérequis

- Rust 1.92+ (uniquement en cas de compilation depuis les sources ; les binaires précompilés n'en ont pas besoin)
- Données stockées dans `~/.research/` (`research.db`, `config.toml`, `embeddings.idx`)
- Les binaires Linux ciblent glibc ≥ 2.38 (Ubuntu 24.04+) ; musl n'est pas fourni faute de binaires précompilés pour le runtime ONNX intégré
- Optionnel : un LLM pour `research gaps` / `research report` **depuis le CLI uniquement** — définissez `[llm]` dans `~/.research/config.toml` (`provider`, `model`, `api_key_env`) ; la clé est lue à l'exécution. Le flux MCP n'en a jamais besoin
- Optionnel : réglage de la recherche hybride via `[search]` dans `~/.research/config.toml` — `provider = "local"` (par défaut, modèle ONNX intégré) ou `"openai"` avec `openai_api_key_env` ; `model` choisit le modèle local (`BGESmallENV15` par défaut, modèle 384-dim compact pour CPU). Voir [ROADMAP.md](../../../ROADMAP.md) pour les éléments non implémentés

## Contribution

Consultez [CONTRIBUTING.md](../../../CONTRIBUTING.md) et [ROADMAP.md](../../../ROADMAP.md). Les pull requests sont les bienvenues.

## Licence

[APACHE-2](../../../LICENSE).
