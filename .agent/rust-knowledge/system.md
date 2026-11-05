# Архитектура, библиотеки и API

Уточнение 2026-10-17: это **целевая** архитектура. Реальный первый срез — [current-slice](current-slice.md): один Rust package, router → use case → repository (SQLx) → PostgreSQL в Docker Compose; `api`, `use_cases`, `repository`, `models`, `crawler`, `config` и два React-экрана. Модели разделены по зонам в models/*.rs. Отдельного модуля pg нет. Транзакция передаётся из router; repository не делает begin/commit. Деление по зонам, файл на use case и соседние errors.rs обязательны. Не создавать будущие модули/зависимости без задачи.

## Поток данных

HTML → raw store/manifest → parser → canonical JSONL + JSON/CSV → Tantivy → Axum API → сайт.

От канонического корпуса отходят фоновые ветки: леммы; классификация → теги; embeddings → Qdrant; извлечённые ссылки/теги → граф. UI читает собственные данные, а не вызывает скрапер при поиске.

## Компоненты

- `domain`: DTO документа, ID, provenance, версии и валидаторы экспорта.
- `ingest`: discovery, robots, общий per-origin scheduler, fetch, parser, resume. HTTP работает только через ограниченный transport.
- `search`: Tantivy schema/analyzers, query planning, BM25, fusion и группировка. Библиотека вызывается API и тестами.
- Служебные команды импорта/экспорта/индексации допустимы как implementation detail. Самостоятельный поисковый CLI не обязателен, `clap` добавлять только при реальной потребности в командах.
- `rks-api`: Axum, валидация HTTP-контрактов, deadlines и admin authorization; тяжёлые синхронные операции не блокируют Tokio executor.
- `web`: React, маршруты поиска/документа/темы/графа/разметки.
- `nlp-worker`: изолированный Python-процесс для spaCy, embeddings и Qwen; не переносить backend и скрапер на Python. Batch JSONL для фоновых работ и локальный HTTP для лемматизации/embedding запроса. Обычный lexical-поиск не зависит от модели.

Начальный каталог: `src/`, `migrations/`, `web/`, `tests/fixtures/`. Разделение на workspace crates, `services/nlp` и `eval` допустимо позже при конкретной необходимости; заранее эти каталоги не создавать. Данные и модели исключены из Git.

## Стек и основания

| Выбор | Назначение и причина |
|---|---|
| Rust, Tokio, Axum | Backend и конвейер обработки, async HTTP и контролируемый параллелизм |
| reqwest + rustls, scraper, url | HTTP, разбор HTML, безопасная нормализация ссылок |
| Проверенный robots-парсер, quick-xml | Правила групп/wildcards и XML sitemap; конкретный robots crate выбрать тестами |
| serde/serde_json, csv | Экспорт с экранированием и общий schema contract |
| Tantivy | Embedded Lucene-подобный индекс на Rust для своего API; не отдельный сервер |
| PostgreSQL + sqlx | Веб-каталог, версии, метки, графовые рёбра, jobs/outbox |
| utoipa + utoipa-swagger-ui | OpenAPI из Rust DTO/handlers и интерактивная проверка API; совместимые версии сверить при реализации |
| Qdrant | Хранение векторов и фильтрация semantic retrieval на позднем этапе |
| spaCy English pipeline | Реальная POS-aware лемматизация; stemmer не засчитывается как lemmatizer |
| Transformers / Sentence Transformers | Ограниченный ML-worker: классификация и embeddings, разные модели |
| React + TypeScript + TanStack Query/Router | Компоненты, типы, серверное состояние и URL-навигация |
| Zustand, fetch | Только локальное UI-состояние; fetch достаточно, Axios не нужен без конкретной причины |
| Cytoscape.js | Интерактивный граф с разными типами узлов/рёбер, выбором и layout |
| tracing, тестовые HTTP origins | Наблюдаемость и проверка политики без нагрузки на сайты |

SQL-очередь достаточна: отдельный брокер, Redis и графовая БД не обязательны для 5–10 тысяч документов. Если они прямо требуются курсом, пересмотреть ADR, а не вводить заранее. Конкретные версии библиотек фиксировать lock-файлами после проверки совместимости.

## Проверка без frontend

Контракт описывается OpenAPI (`/api/openapi.json`), Swagger UI доступен в dev (`/swagger-ui`). Сервер реализован для каталога/статистики; `/api/search` в примерах ниже — будущий endpoint, пока отсутствует:

```sh
curl --get http://localhost:8080/api/search --data-urlencode 'q=generic associated types' --data-urlencode 'mode=lexical' --data-urlencode 'limit=10'
curl --get http://localhost:8080/api/search --data-urlencode 'q=ограничения async' --data-urlencode 'mode=hybrid'
curl http://localhost:8080/api/openapi.json
```

Ответ содержит rank, document ID, title, URL, score, snippet, source, original/effective query, mode, generation и degradation. Empty results — успешный HTTP-ответ, неверный input — 4xx, недоступный обязательный индекс — 503. Lexical-поиск не зависит от Qwen/Qdrant. API-тесты проверяют ту же поисковую библиотеку, которой пользуется сайт.

Обратный индекс демонстрируется интеграционным тестом или dev/admin inspect endpoint: ограниченное число postings (document ID, frequency, positions) и df. В публичном API диагностические внутренности не обязательны. Не писать второй собственный поисковый движок.

Веб-БД синхронизируется с версионированными exports. Обновление документа и событие outbox атомарны; повтор обработки идемпотентен. API проверяет актуальность документа при hydration.
