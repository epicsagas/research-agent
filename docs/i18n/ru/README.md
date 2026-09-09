<!-- Translated from README.md @ commit f2c178a (2026-09-10) -->
<!-- If English README has changed since then, this translation may be outdated -->

> Этот документ является переводом [README.md](../../../README.md).
> Английская версия является официальным источником (source of truth) и может быть более актуальной.

<div align="center">

# research-agent

> Долговременная исследовательская память для ИИ-агентов — индексация статей, поиск пробелов в знаниях, генерация обзоров

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
  <b>Русский</b> |
  <a href="../it/README.md">Italiano</a>
</p>

</div>

## Что это такое?

research-agent — это **исследовательский сервер для ИИ-агентов**: он индексирует научные статьи (arXiv, Semantic Scholar, локальные PDF) в локальную библиотеку SQLite, а ваш агент напрямую управляет им по протоколу MCP — загружает статьи, выполняет поиск, отслеживает прогресс чтения, анализирует пробелы в покрытии темы с помощью собственной модели и формирует обзорные отчеты. Ключ API не требуется.

Этот же бинарный файл может использоваться как автономный CLI для терминала и скриптов.

## Возможности

| | Возможность | Почему это важно |
|--|------------|------------------|
| 🤖 | MCP-сервер | 16 инструментов, которыми ваш агент управляет напрямую через stdio — без ключей API |
| 🧠 | Нативный анализ агента | Анализ пробелов и создание отчетов выполняются внутри агента: инструменты передают структурированное состояние, агент рассуждает, результаты сохраняются |
| 🔗 | Граф цитирований | Связи между статьями из OpenAlex (прямые и обратные), с опциональной разметкой намерений цитирования от Semantic Scholar |
| 📚 | Индексация статей | arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed), препринты bioRxiv, локальные PDF и Zotero (запущенный клиент или файлы экспорта) |
| 🔍 | Гибридный поиск | Лексический FTS5 + локальный семантический поиск ONNX, объединенные с помощью RRF — работает полностью офлайн |
| 📂 | Дерево тем | Иерархическая организация исследований с помощью подтем |
| 📖 | Трекер чтения | Очередь чтения, отслеживание статуса и выставление оценок прочитанному |
| ⚡ | Единый бинарный файл | Никаких дополнительных сред выполнения или внешних серверов — только `research` |

## Быстрый старт

Установите плагин в вашего ИИ-агента — тулчейн Rust не требуется:

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

Hermes загружает корневой `plugin.yaml` и функцию `register(ctx)` в `__init__.py`. Поскольку Hermes не поддерживает MCP, агент управляет `research` через команды CLI встроенного скилла — предварительно установите бинарный файл (`brew install epicsagas/tap/research-agent` или скриптом curl); хук SessionStart не применяется к Hermes. Если skills_guard блокирует проверку установки, укажите `plugins.scan_on_install: false` в конфигурации Hermes.

Плагин автоматически устанавливает бинарный файл `research` при старте сессии и предоставляет 16 инструментов MCP, позволяя агенту автономно собирать, искать, анализировать и сохранять отчеты с помощью собственной модели.

После установки попробуйте попросить агента:

- "Собери недавние статьи по обучению графовых нейронных сетей из arXiv"
- "Какие пробелы в знаниях остались по теме <topic>?"
- "Сгенерируй обзорный отчет по теме <topic>"

## Как агент использует сервис

Плагин автоматически устанавливает бинарный файл `research` при старте сессии и запускает **stdio MCP-сервер** (`research mcp`) — агент самостоятельно находит и вызывает инструменты без необходимости вводить команды вручную.

**Инструменты** (16): `init` · `ingest` · `index_rebuild` · `import_papers` · `paper_body` · `paper_references` · `query_papers` · `topic_brief` · `gaps_record` · `list_gaps` · `report_material` · `report_save` · `topics_list` · `topic_add` · `state` · `update_read`.

Анализ является **нативным для агента**: `topic_brief` и `report_material` передают структурированное состояние библиотеки (статьи, прогресс чтения, записанные пробелы, покрытие темы), агент делает выводы с помощью своей модели, а `gaps_record` / `report_save` сохраняют результаты. При анализе текста инструмент `paper_body` принимает опциональный параметр `query` для возврата точных фрагментов с указанием раздела и номера страницы вместо всего текста, что существенно экономит контекст. Никакой настройки `[llm]` или внешнего ключа API для работы MCP не требуется.

Проверка работоспособности сервера через чистый JSON-RPC:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | research mcp 2>/dev/null
```

Cargo-функция `mcp` **включена по умолчанию**; собрать версию только с CLI можно командой `cargo build --no-default-features`.

## Автономный CLI (второстепенно)

Предпочитаете ручное управление? Тот же бинарный файл работает как классический CLI.

```bash
# macOS / Linux — готовый бинарный файл, Rust не требуется
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/epicsagas/research-agent/releases/latest/download/install.sh | sh

# Windows — готовый бинарный файл, Rust не требуется
irm https://github.com/epicsagas/research-agent/releases/latest/download/install.ps1 | iex

# Homebrew (macOS / Linux)
brew install epicsagas/tap/research-agent
```

### Первоначальная настройка

Выполните `research init` в терминале — мастер интерактивно проведет вас по ключевым параметрам: расположение базы данных, поставщик LLM для анализа пробелов и отчетов (ключ считывается из указанной переменной окружения и не сохраняется в файле), а также модель эмбеддингов для гибридного поиска (с возможностью немедленной загрузки). Локальные серверы (Ollama, LM Studio) опрашиваются на предмет уже загруженных моделей. Повторный запуск безопасен: существующие значения становятся значениями по умолчанию и ничего не сбрасывается.

Все настраиваемые параметры находятся в `~/.research/config.toml`: незаданные опции отображаются закомментированными со значениями по умолчанию. Флаг `research init --no-onboard` пропускает интерактивные запросы (это также происходит автоматически, если stdin не является терминалом).

## Обновление

| Способ установки | Команда |
|------------------|---------|
| Скрипт установки curl (macOS/Linux) | Повторно запустите скрипт установки, указанный выше |
| Скрипт PowerShell (Windows) | Повторно запустите команду установки, указанную выше |
| Homebrew | `brew upgrade epicsagas/tap/research-agent` |

Проверка установленной версии:

```bash
research --version
```

## Список команд

| Команда | Описание |
|---------|----------|
| `research init` | Инициализация рабочего пространства (интерактивный мастер в терминале: выбор провайдера, переменной окружения для ключа, модели эмбеддингов + загрузка; `--no-onboard` для пропуска) |
| `research ingest <query> [--source arxiv\|s2\|openalex\|europepmc\|preprints\|all] [--limit <n>] [--topic <id>]` | Загрузка статей из arXiv, Semantic Scholar, OpenAlex, Europe PMC (PubMed) или серверов препринтов (bioRxiv, medRxiv и др.); можно сразу связать с темой |
| `research ingest [--source zotero] [query] [--topic <id>]` | Чтение статей из запущенного Zotero через локальный API (`ZOTERO_BASE_URL` переопределяет адрес по умолчанию, необходимо включить "Allow other applications on this computer to communicate with Zotero"); без запроса загружается вся библиотека. Не входит в `--source all` |
| `research ingest --source pdf --path <file\|dir> [--topic <id>]` | Загрузка локальных PDF-файлов (полный текст сохраняется и доступен для поиска) |
| `research import <file\|dir>` | Импорт файлов BibTeX/BibLaTeX, CSL-JSON или Zotero JSON (экспорт приложения или API-формат) с аннотациями, тегами и нормализованными DOI; существующие статьи пропускаются |
| `research index [--rebuild]` | Создание или перестроение поискового индекса (FTS + векторный индекс) |
| `research reingest [--missing-pages]` | Повторное извлечение текста сохраненных PDF (добавляет маркеры страниц) |
| `research query <q> [--evidence]` | Поиск статей — гибридный лексический+семантический поиск при наличии эмбеддингов; `--evidence` показывает фрагменты текста с номерами разделов и страниц |
| `research references <id> [--cited-by] [--intents]` | Получение ребер графа цитирования из OpenAlex; `--cited-by` меняет направление, `--intents` добавляет метки намерения цитирования от Semantic Scholar |
| `research gaps [--topic <id>]` | Анализ пробелов в знаниях (в CLI: использует `[llm]`, если настроен) |
| `research report --topic <id>` | Генерация исследовательского обзора (в CLI: использует `[llm]`, если настроен) |
| `research topics list` | Список всех тем исследований |
| `research topics add <name> [--parent <id>]` | Добавление новой темы (укажите `--parent` для создания подтемы) |
| `research read <id> [--status <status>] [--rating <1-5>]` | Обновление статуса чтения или рейтинга статьи |
| `research read <id> --body` | Вывод сохраненного текста статьи |
| `research status` | Общий обзор состояния исследований |
| `research mcp` | Запуск stdio MCP-сервера (псевдоним: `serve`) |

Каждая подкоманда поддерживает глобальный флаг `--db <path>`, позволяющий задать конкретный файл базы данных вместо `~/.research/research.db` — удобно для тестовых или изолированных сред.

## Системные требования

- Rust 1.92+ (требуется только при сборке из исходного кода; готовые бинарные файлы не требуют Rust)
- Данные сохраняются в `~/.research/` (`research.db`, `config.toml`, `embeddings.idx`)
- Бинарные файлы для Linux собраны под glibc ≥ 2.38 (Ubuntu 24.04+); musl не поддерживается из-за отсутствия готовых сборок встроенного ONNX Runtime для musl
- Опционально: LLM для команд `research gaps` / `research report` **только при вызове из CLI** — настройте `[llm]` в `~/.research/config.toml` (`provider`, `model`, `api_key_env`); ключ читается из переменной окружения в момент вызова. Для MCP это не требуется
- Опционально: настройка гибридного поиска через `[search]` в `~/.research/config.toml` — `provider = "local"` (по умолчанию, встроенная модель ONNX) или `"openai"` с параметром `openai_api_key_env`; `model` выбирает локальную модель (по умолчанию `BGESmallENV15`, компактная 384-мерная модель для CPU). См. [ROADMAP.md](../../../ROADMAP.md)

## Участие в разработке

См. [CONTRIBUTING.md](../../../CONTRIBUTING.md) и [ROADMAP.md](../../../ROADMAP.md). Pull Request'ы приветствуются.

## Лицензия

[APACHE-2](../../../LICENSE).
