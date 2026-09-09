<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Este documento es una traducción de [README.md](../../../README.md).
> La versión en inglés es la fuente oficial (source of truth) y puede estar más actualizada.

<div align="center">

# research-agent

> Memoria de investigación a largo plazo para agentes de IA — artículos indexados, brechas detectadas, informes generados

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
  <b>Español</b> |
  <a href="../fr/README.md">Français</a> |
  <a href="../de/README.md">Deutsch</a> |
  <a href="../pt/README.md">Português</a> |
  <a href="../ru/README.md">Русский</a> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## ¿Qué es esto?

research-agent es un **servidor de investigación para agentes de IA**: indexa artículos científicos (arXiv, Semantic Scholar, PDFs locales) en una biblioteca SQLite local, y su agente lo controla directamente a través de MCP — ingiriendo, buscando, siguiendo el progreso de lectura, analizando brechas de cobertura con su propio modelo y guardando informes de revisión. No requiere ninguna clave de API.

El mismo binario también funciona como una CLI independiente para terminales y scripts.

## Características principales

| | Característica | Por qué es importante |
|--|----------------|-----------------------|
| 🤖 | Servidor MCP | 16 herramientas que su agente controla directamente a través de stdio — sin necesidad de claves de API |
| 🧠 | Análisis nativo de agentes | El análisis de brechas y los informes se ejecutan dentro de su agente: las herramientas entregan un estado estructurado, el agente razona y los resultados se guardan |
| 🔗 | Grafo de citas | Relaciones de referencias entre artículos desde OpenAlex, directas e inversas, opcionalmente etiquetadas con intenciones de cita de Semantic Scholar |
| 📚 | Indexación de artículos | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), preprints estilo bioRxiv, PDFs locales y Zotero (instancia en ejecución o archivos exportados) |
| 🔍 | Búsqueda híbrida | Búsqueda léxica FTS5 + semántica ONNX local, fusionadas mediante RRF — funciona sin conexión |
| 📂 | Árboles de temas | Organice la investigación de forma jerárquica con subtemas |
| 📖 | Seguimiento de lectura | Cola de lectura, seguimiento del progreso y calificación de lo leído |
| ⚡ | Binario único | Sin tiempos de ejecución adicionales ni servidores externos — solo `research` |

## Inicio rápido

Instálelo como complemento en su agente de IA — no requiere la cadena de herramientas de Rust:

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

Hermes carga `plugin.yaml` en la raíz y `register(ctx)` en `__init__.py`. Al no admitir MCP, el agente ejecuta `research` mediante los comandos de la CLI incluidos en la habilidad. Instale primero el binario (`brew install epicsagas/tap/research-agent` o el instalador curl); el hook de autoinstalación SessionStart no se aplica a Hermes. Si skills_guard bloquea el escaneo de instalación, configure `plugins.scan_on_install: false` en la configuración de Hermes.

El plugin instala automáticamente el binario `research` al iniciar la sesión y expone 16 herramientas MCP, lo que permite al agente recopilar, buscar, analizar con su propio modelo y generar informes por sí mismo.

Una vez instalado, puede pedirle a su agente cosas como:

- "Recopila artículos recientes sobre entrenamiento de Graph Neural Networks desde arXiv"
- "¿Qué brechas de conocimiento quedan en mi cobertura de <topic>?"
- "Genera un informe de revisión sobre <topic>"

## Cómo lo utiliza su agente

El plugin instala automáticamente el binario `research` al inicio de la sesión e inicia el **servidor MCP stdio** (`research mcp`) — el agente descubre y llama a las herramientas directamente, sin que un humano tenga que escribir comandos de CLI.

**Herramientas** (16): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read`.

El análisis es **nativo del agente**: `topic_brief` y `report_material` proporcionan el estado estructurado de la biblioteca (artículos, progreso de lectura, brechas registradas, cobertura), el agente razona sobre ello con su propio modelo, y `gaps_record` / `report_save` guardan los hallazgos. Al inspeccionar textos, `paper_body` acepta un parámetro opcional `query` para devolver fragmentos de evidencia anclados (con sección y página) en lugar de todo el cuerpo, ahorrando contexto. No se requiere configuración de `[llm]` ni claves de API en todo el flujo de MCP.

Comprobación básica del servidor mediante JSON-RPC en crudo:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

La característica cargo `mcp` está **habilitada por defecto**; compile únicamente la CLI con `cargo build --no-default-features`.

## CLI independiente (secundaria)

¿Prefiere utilizarlo manualmente? El mismo binario funciona como una CLI tradicional.

```bash
# macOS / Linux — binario precompilado, sin necesidad de Rust
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — binario precompilado, sin necesidad de Rust
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Configuración inicial

Ejecute `research init` en un terminal y le guiará a través de las opciones clave: ubicación de la base de datos, el proveedor de LLM utilizado para el análisis de brechas e informes (la clave se lee de la variable de entorno que especifique, nunca se guarda en el archivo), y el modelo de embedding para la búsqueda híbrida, con la opción de descargarlo en ese mismo instante. Detecta servidores locales (Ollama, LM Studio) para mostrar los modelos ya cargados y permitirle elegir entre los realmente disponibles. Volver a ejecutarlo es seguro: los valores existentes se convierten en los predeterminados y nada se restablece.

Todo lo configurable aparece en `~/.research/config.toml`: las opciones no modificadas aparecen comentadas con sus valores predeterminados, actuando como su propia documentación. `research init --no-onboard` omite las preguntas interactivas (también ocurre de forma automática cuando stdin no es una terminal, para no bloquear scripts).

## Actualización

| Método | Comando |
|--------|---------|
| Instalador curl (macOS/Linux) | Vuelva a ejecutar el script de instalación anterior |
| Instalador PowerShell (Windows) | Vuelva a ejecutar el comando de instalación anterior |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Comprobar la versión instalada:

```bash
research --version
```

## Comandos

| Comando | Descripción |
|---------|-------------|
| `research init` | Inicializa el espacio de trabajo (configuración interactiva en terminal: proveedor, nombre de variable de entorno, modelo de embedding + descarga; `--no-onboard` para omitir) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | Recopila artículos de arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) o servidores de preprints (bioRxiv, medRxiv, …); opcionalmente vincula directamente a un tema |
| `research ingest [--source zotero] [query] [--topic <id>]` | Lee artículos de una instancia de Zotero en ejecución a través de su API local (`ZOTERO_BASE_URL` anula el endpoint predeterminado, requiere activar "Permitir que otras aplicaciones se comuniquen con Zotero"); sin consulta obtiene toda la biblioteca. No incluido en `--source all` |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | Recopila archivos PDF locales (el texto completo se almacena y se puede buscar) |
| `research import <file\|dir>` | Importa archivos BibTeX/BibLaTeX, CSL-JSON o Zotero JSON (exportación de escritorio o formato API), con resúmenes, etiquetas y DOIs normalizados; omite los artículos ya existentes |
| `research index [--rebuild]` | Crea o reconstruye el índice de búsqueda (FTS + índice vectorial) |
| `research reingest [--missing-pages]` | Re-extrae los textos de PDFs almacenados (agrega marcadores de página a cuerpos indexados antes de su compatibilidad) |
| `research query <q> [--evidence]` | Busca artículos — híbrido léxico+semántico cuando los embeddings están disponibles; `--evidence` muestra también el texto coincidente con su sección y página |
| `research references <id> [--cited-by] [--intents]` | Obtiene relaciones del grafo de citas desde OpenAlex; `--cited-by` invierte la dirección, `--intents` añade etiquetas de intención de cita de Semantic Scholar |
| `research gaps [--topic <id>]` | Analiza brechas de conocimiento (CLI: utiliza `[llm]` si está configurado) |
| `research report --topic <id>]` | Genera un informe de revisión (CLI: utiliza `[llm]` si está configurado) |
| `research topics list` | Lista todos los temas |
| `research topics add <name> [--parent <id>]` | Agrega un nuevo tema (especifique `--parent` para crear un subtema) |
| `research read <id> [--status <status>] [--rating <1-5>]` | Actualiza el estado de lectura o la calificación |
| `research read <id> --body` | Imprime el texto completo almacenado de un artículo |
| `research status` | Muestra el resumen del estado de la investigación |
| `research mcp` | Inicia el servidor MCP stdio (alias: `serve`) |

Cada subcomando acepta también la opción global `--db <path>` para usar una base de datos específica en lugar de `~/.research/research.db` — ideal para espacios de prueba o entornos aislados.

## Requisitos

- Rust 1.92+ (solo si se compila desde el código fuente; los binarios precompilados no lo necesitan)
- Los datos se almacenan en `~/.research/` (`research.db`, `config.toml`, `embeddings.idx`)
- Los binarios para Linux están orientados a glibc ≥ 2.38 (Ubuntu 24.04+); musl no está disponible porque el runtime ONNX incluido no tiene binarios para musl
- Opcional: un LLM para `research gaps` / `research report` **únicamente desde la CLI** — configure `[llm]` en `~/.research/config.toml` (`provider`, `model`, `api_key_env`); la clave se leerá de esa variable en el momento de la llamada. El flujo MCP nunca lo necesita
- Opcional: ajuste de la búsqueda híbrida mediante `[search]` en `~/.research/config.toml` — `provider = "local"` (predeterminado, modelo ONNX incluido) u `"openai"` con `openai_api_key_env`; `model` selecciona el modelo local (por defecto `BGESmallENV15`, modelo ligero de 384 dimensiones que corre en CPU). Consulte [ROADMAP.md](../../../ROADMAP.md) para ver lo que deliberadamente aún no está implementado

## Contribuir

Consulte [CONTRIBUTING.md](../../../CONTRIBUTING.md) y [ROADMAP.md](../../../ROADMAP.md). Los Pull Requests son bienvenidos.

## Licencia

[APACHE-2](../../../LICENSE).
