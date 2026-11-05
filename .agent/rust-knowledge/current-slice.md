# Текущая архитектура: сбор и просмотр

Актуально: **2026-10-17**. Рабочая директория: `/home/aragami3070/ssu/term7/prog-math-solutions`.

Это реализованный первый срез, а не обещание готовности полного проекта. Последнее требование пользователя: Axum + скрапер, список собранных страниц с пагинацией и отдельная статистика; не создавать неиспользуемые модули. При расхождении с ранним full-product планом именно этот документ определяет текущий объём.

## Решение

Один Rust package, один серверный процесс и PostgreSQL в Docker Compose. Цепочка backend по решению пользователя: **router → use case → repository → PostgreSQL через SQLx**. Отдельного модуля `pg` нет. Фоновая задача сборщика живёт в процессе. React работает с собственным JSON API; открытие страницы не обращается к внешним источникам. После `npm run build` Axum раздаёт SPA и поддерживает прямой переход `/stats`.

Настройки локальной разработки — `.env` в корне проекта, шаблон `.env.example`. `main` загружает файл через dotenvy до создания Tokio runtime и настройки tracing. Уже заданные environment variables имеют приоритет; изменения файла применяются при рестарте. Отсутствующий файл допустим, ошибки чтения/синтаксиса останавливают запуск без печати секретных строк. `.env` не коммитится.

По решению пользователя локальный API работает без авторизации. `HTTP_BIND` валидируется как loopback (`127.0.0.1` или `::1`), по умолчанию `127.0.0.1:8080`. Токены и заголовки Authorization не требуются. Защита административных операций для внешнего развёртывания остаётся дальнейшей задачей.

```text
src/
  main.rs           сборка зависимостей, миграции, запуск, shutdown
  lib.rs            доступ к тем же модулям из тестов
  config.rs         окружение и безопасные defaults
  models/
    sources.rs      Source, DocumentSummary, DocumentPage, ParsedDocument
    crawls.rs       CrawlRequest, CrawlRun, FetchRecord, CrawlPlan, QueuedPage, CollectedPage
    statistics.rs   SourceStats, Statistics
    pagination.rs   общая Pagination
    exports.rs      ExportQuery, ExportDocument, ExportRow, ExportArtifact
    mod.rs          объявления модулей
  api/
    mod.rs          AppState, сборка routers, OpenAPI, SPA
    crawls.rs       HTTP-запуск сбора
    sources.rs      список документов и возобновление источника
    statistics.rs   HTTP-статистика
    health.rs       readiness
    errors.rs       HTTP-ошибки и преобразование ошибок use cases
    tests.rs        API-контракты без внешних запросов
  use_cases/
    crawls/         start, record_fetch, save_document, record_failure,
                    finish, recover, check_lease, next_page, complete_page, fail_page — каждый в своём .rs
    sources/        list_documents.rs, resume.rs, export_corpus.rs
    statistics/     get.rs
    health/         check.rs
                    в каждой зоне свои mod.rs и errors.rs
  repository/
    crawls.rs       SQL запусков, raw fetches, счётчиков и advisory lock
    sources.rs      SQL документов и состояния источников
    statistics.rs   SQL HTTP-счётчиков
    health.rs       SQL проверки соединения
    errors.rs       ошибки доступа к PostgreSQL
    mod.rs          объявления модулей
  crawler/
    mod.rs          запуск, жизненный цикл и координация сбора
    discovery.rs    HTML-ссылки, scope, нормализация и seeds
    http.rs         единый ограниченный transport + robots
    sources.rs      allowlist и чистые HTML-парсеры Internals/RFC
    persistence.rs  границы транзакций фонового worker → use cases
    errors.rs       ошибки остановки источника
migrations/         схема PostgreSQL
tests/fixtures/     синтетические HTML-примеры
tests/storage.rs    изолированный SQL integration test
web/src/            два React-экрана, DTO/API-client, стили, тесты
```

Router декодирует HTTP, вызывает use case и преобразует его ошибки в HTTP-ответ. Бизнес-правила и композиция операций — в use cases; SQL — только в repository. Документы относятся к `sources` на всех трёх слоях; существующий URL `GET /api/documents` сохранён. Parser получает HTML и URL, возвращает `ParsedDocument`, не делает I/O. Новые неиспользуемые модули не создавать.

### Обязательные правила дальнейших изменений

- Делить api/repository по зонам ответственности. В use_cases для каждой зоны отдельный каталог и файл на use case.
- Модели делить по зонам в `models/*.rs`: документы относятся к sources, запуски/загрузки к crawls, статистика к statistics, общая пагинация отдельно. Импортировать из конкретного модуля, без общего model.rs и wildcard re-exports.
- SQL писать макросами `sqlx::query!`, `sqlx::query_as!`, `sqlx::query_scalar!`, включая запросы в интеграционных тестах. SQL оформлять многострочными raw strings с читаемыми SELECT/SET/JOIN/WHERE и отдельными аргументами. Utoipa path/params/responses также разносить по строкам. Типы и nullable-результаты проверяются при компиляции; обязательность агрегатов отмечается обоснованными алиасами `"field!"`.
- Offline-метаданные лежат в `.sqlx/` и включаются в Git. `SQLX_OFFLINE=true` задан в `.env.example` и локальном `.env`. После изменения SQL/схемы применить миграции и выполнить `cargo sqlx prepare -- --all-targets --locked`; актуальность проверить `cargo sqlx prepare --check -- --all-targets --locked`. Команды CLI используют `DATABASE_URL` из `.env`. Offline-режим касается компиляции, а не работы приложения с БД.
- Объявлять типы ошибок в соседнем `errors.rs` на том же уровне, что код, из которого они вынесены: `api/errors.rs`, `repository/errors.rs`, `use_cases/sources/errors.rs`, `crawler/errors.rs` и т. д.
- Для работы с БД router открывает транзакцию и передаёт `&mut Transaction<'_, Postgres>` в use case. Use case передаёт ту же транзакцию в repository. Ни use case, ни repository не открывают транзакций, не получают pool и не выполняют commit.
- Router выполняет commit после успешного use case; при ошибке транзакция откатывается. Чтение каталога/статистики использует read-only repeatable-read snapshot.
- Use case без работы с БД не требует транзакции. Например, проверка crawl request и подготовка плана выполняются до получения соединения. Интерфейс будущего Qwen-worker решать при реализации, заранее его не создавать.
- Фоновый скрапер имеет собственную входную границу `crawler/persistence.rs`: она открывает короткую транзакцию и вызывает тот же слой use cases. Сетевая загрузка HTML идёт после завершения транзакции; соединение с session advisory lock удерживается отдельно.
- Запуск worker допускается только после commit записи crawl run. Сохранение документа и succeeded атомарно; paused source и blocked run также сохраняются в одной транзакции. Тест проверяет общий rollback нескольких use cases.

## Уже нужные зависимости

| Библиотека | Зачем в текущем коде |
|---|---|
| Tokio + Axum | Async HTTP, фоновый crawl, сигналы завершения |
| tokio-util | CancellationToken и TaskTracker: задачи не теряются при штатном shutdown |
| tower-http | HTTP trace, статические файлы React |
| PostgreSQL + sqlx | Миграции, транзакции, списки/статистика, upsert, advisory lock |
| reqwest + rustls | HTTPS с явными timeout, без автоматических redirects/proxy |
| robotstxt | Allow/Disallow, группы User-Agent и wildcard rules; не самописный substring matcher |
| scraper + url | Извлечение основного HTML, нормализация и allowlist URL |
| serde + serde_json, chrono, sha2 | DTO/metadata JSON, UTC timestamps, hash нормализованного текста |
| utoipa + swagger-ui | OpenAPI из handlers/DTO; vendored UI без build-time загрузки Swagger ZIP |
| tracing, anyhow, thiserror | Диагностика, контекст ошибок и типизированные причины отказа |
| dotenvy | Автоматическая загрузка локального `.env` при старте |
| React/TS + Vite | Два экрана и локальная разработка |
| TanStack Query/Router + fetch | Серверный cache, отмена запросов, URL-пагинация, навигация |
| Vitest | Малые frontend unit tests; конкретные версии закреплены lock-файлом |

Zustand пока не нужен: выбор источника хранится в useState, запрос запуска — TanStack mutation. Axios, Redis, Kafka, Tantivy, Qdrant, ML dependencies и Kubernetes не добавлены.

## Сбор: последовательность и границы

1. `POST /api/crawls` принимает только `{"source":"internals"}` или `{"source":"rfc"}`. Список URL и лимит 20 отменены. Начальный URL — главная выбранного ресурса.
2. Dedicated PostgreSQL connection получает session advisory lock. Уже есть сбор → `409`, paused source → `409`. Закрытие этой connection освобождает lock; её нельзя вернуть в pool с удерживаемой блокировкой.
3. Use case продолжает незавершённый run с очередью либо создаёт running run и начальную страницу в транзакции router. Только после commit router запускает worker и возвращает `202`. Сбор выполняется отслеживаемой фоновой задачей; UI не ждёт завершения HTTP-запроса.
4. Запрос `/robots.txt`. Ответ сохраняется отдельно от document requests. Robots `404` допускает дальнейшую проверку URL; 200 HTML/timeout/прочие статусы останавливают текущий запуск. Действующая политика сверяется заново при следующем запуске.
5. Последовательные requests с задержкой ≥5 секунд **до каждого** запроса. Максимальный допустимый объявленный crawl-delay увеличивает задержку; неразбираемое или >300 секунд значение требует ручной проверки. Таймаут соединения 10 секунд, общий HTTP timeout 25 секунд; decoded body ≤2 MiB. Redirects вручную, не более четырёх запросов на документ, только тот же origin и нормализованная идентичность страницы, включая номер пагинации, robots сверяется снова.
6. Временные сбои (connect/timeout/ошибка чтения тела, 502/503/504) — до 3 попыток с backoff `delay·2^n` (10 и 20 с при задержке 5 с), каждая попытка тоже ждёт обычную паузу и записывается в `fetches`. Пять страниц подряд, оставшиеся недоступными (`Unavailable`), останавливают запуск как `failed` без пометки остальной очереди ошибочной. `403`/`429` и известный challenge → немедленный stop + persistent source pause. `Retry-After` сохраняется в причине; возобновление только оператором после проверки/ожидания. Не делаем автоматических retry и не обещаем отсутствие бана.
7. Raw HTML → извлечение HTML-ссылок → parser → upsert по `(source, native_id)`. Сохранение документа, discovered URLs, счётчика и отметки страницы done — одна транзакция. На ошибке парсера найденные допустимые ссылки всё равно попадают в очередь. Страницы навигации не создают документы; у тем Internals объединяются страницы текущего запуска и дедуплицируются посты. Ошибка парсинга не сохраняет мусор как документ и не уничтожает предыдущий документ; исходный ответ остаётся в fetches. Ошибка отдельного документа отмечается, остальные разрешённые URL могут обрабатываться дальше.
8. Run становится completed/failed/blocked/interrupted. Штатный shutdown отменяет crawl; при старте под свободным lock потерянные running rows становятся interrupted. Другой активный сборщик при этом не прерывается.

Глобальный lock намеренно консервативнее per-origin scheduler: даже Internals и RFC сейчас не собираются параллельно. Очередь durable, но конкурентного выполнения заданий несколькими workers пока нет. Ограничение действует только для процессов с одной БД, а не для независимых инсталляций.

## Данные

- `crawl_sources`: два источника, paused flag и причина, переживают рестарт.
- `crawl_runs`: статус, requested/succeeded/failed, время, последняя причина ошибки. `requested` — число обнаруженных и поставленных в очередь HTML-страниц (растёт в ходе обхода), `succeeded` — успешно обработанные страницы, включая навигацию; `failed` — ошибки страниц. Это не число уникальных документов. При ручном продолжении failed сбрасывается, страницы повторяются, raw история сохраняется.
- `crawl_queue`: run_id, `url` (каноническая идентичность страницы, без slug темы), `fetch_url` (что реально запрашивается: со slug темы, чтобы Discourse не отвечал 301 на каждую страницу; slug берётся из ссылки, найденной на странице), pending/done/failed, error. UNIQUE(run_id, url) устраняет циклы; обход в порядке id (FIFO). Страница остаётся pending до атомарного checkpoint; незавершённая загрузка может повториться после рестарта. Повторный POST продолжает failed/interrupted/blocked run с незавершённой очередью, если источник не paused. После completed начинает новый обход.
- Повторное использование: страница документа со свежей строкой `document_pages` (моложе `CRAWL_FRESH_DAYS`, документ разобран текущим `PARSER_VERSION`) не запрашивается; её ссылки берутся из `fetches.body`. Навигационные страницы всегда качаются.
- `fetches`: URL, kind robots/document, timestamp, HTTP status/content type, raw response/error, run ID. Robots, redirects и повторные загрузки — самостоятельные события. Тела блокировок/ошибок чтения могут отсутствовать. Данные помещаются в PostgreSQL TOAST; отдельного S3 пока нет.
- `documents`: тема или RFC; source/native ID, canonical URL, title, производные `body_text`/hash/completeness (+причина), parser version, `metadata` только с license_source, first/last seen. Старые normalized versions пока не ведутся, raw сохраняются.
- `document_pages`: одна строка на HTTP-страницу документа (`page_no`, url, fetch_id, `has_next`, fetched_at), UNIQUE(document_id, page_no). Страницы копятся между запусками; перезагрузка заменяет только свою строку.
- `posts`: один пост (post_number, author, published_at, text, code JSONB), UNIQUE(document_id, post_number); переехавший пост переназначается на новую страницу. У RFC — один пост с текстом `main`.
- `links`: ссылки поста (target_url, anchor_text), UNIQUE(post_id, target_url). Включают якорные ссылки на сноски и на ту же тему; для графа их надо отфильтровать или классифицировать позже.
- Полнота считается чистой функцией `Completeness::evaluate` по `(page_no, has_next)` всех страниц документа. Источник правды — страницы и посты, не поле `completeness`.

Авторы, даты и номера постов, код и ссылки лежат в `posts`/`links`, ссылка на условия лицензирования — в `metadata`. Классификация лицензии каждого поста остаётся следующей задачей. `id` BIGSERIAL используется UI; логическая межсистемная идентичность — `source:native_id`.

## Два экрана и контракт

| Маршрут | Поведение |
|---|---|
| `/` | Список, source/ID/title/excerpt/дата сбора/объём/полнота; ссылка на оригинал |
| `/stats` | Всего документов; HTTP-запросы к документам/ошибки; разбивка по двум источникам; последние 10 runs |
| `GET /api/health` | 204 при доступной БД, 503 при отказе |
| `GET /api/documents?page=1&page_size=20` | items + total/total_pages/page/page_size |
| `GET /api/stats` | Summary, sources, recent_runs |
| `GET /api/sources/export` | Снимок корпуса: JSONL/JSON/CSV/ZIP, source/complete_only/spreadsheet_safe |
| `POST /api/crawls` | source → 202 run; HTML-обход или продолжение очереди |
| `POST /api/sources/{source}/resume` | Ручное снятие паузы → 204, без автозапуска |
| `/api/openapi.json`, `/swagger-ui/` | Машинный контракт и ручная проверка |

Pagination: API page 1..1 000 000, page_size 1..100, UI варианты 20/50/100. Сортировка `first_seen_at DESC, id DESC`; повторный сбор не прыгает наверх. Count и items читаются в одном repeatable-read snapshot. Между разными HTTP-запросами при добавлении документов offset-пагинация может сдвинуть границу страниц — cursor/snapshot pagination пока не нужна. Выход за последнюю страницу возвращает пустые items и актуальный total; UI предлагает первую страницу. Неверные типы/границы → 400.

Статистика также читает единый snapshot. `http_errors` относится только к document fetches: network/read errors или HTTP ≥400. Robots failures и parser errors отдельно видны в run status/error; total документов не включает raw fetches. `text_chars` — символы текста, **не байты хранения**.

URL хранит page/page_size. Query cache не дублируется в Zustand. Polling: список 15 секунд, статистика 5 секунд только своей системы, не источников. Есть loading, empty, error с retry, кнопка refresh. React отображает текст без `dangerouslySetInnerHTML`; raw HTML не исполняется. На главной странице выбор источника, запуск/продолжение и прогресс очереди; кнопка заблокирована при активном сборе или paused source. Ошибки API выводятся пользователю; POST автоматически не повторяется. Также доступны curl/Swagger.

## Как расширять, когда понадобится

Экспорт v1 уже реализован в зоне sources: router открывает repeatable-read read-only транзакцию, use case читает repository порциями по 32 документа, сериализует во временные файлы, router делает commit и отдаёт файл через ReaderStream. Весь корпус в RAM не загружается; параллельная подготовка в одном процессе ограничена одним экспортом. ZIP содержит JSONL/JSON/CSV и manifest с count/hash/filters. Временное хранилище — tempfile, CSV — библиотека csv, ZIP — zip без сжатия, потоковая отдача — tokio-util/io. Выбор источника и режима экспорта доступен внизу страницы статистики; скачивание нативное, без большого Blob в JS. Ошибки API при скачивании показываются в отдельной вкладке. Полный контракт и пробелы provenance — [data](data.md).

1. Сначала live-пилот, доказанная полнота Internals и полный provenance. Не засчитывать synthetic fixtures/partial темы в 5 000.
2. Экспорт JSONL/JSON/CSV/ZIP уже есть; Дальше — разбиение RFC по заголовкам; сырой HTML в `fetches` позволяет при смене парсера пересобрать документы без повторного обхода (команды для этого нет, при необходимости написать заново). Round-trip, фильтры, checksums и стабильность снимка проверены тестами.
3. Tantivy как новая проекция документов и отдельный модуль, не часть парсера. На этом этапе добавить outbox/версионирование для атомарного обновления и перестройки индекса.
4. При необходимости durable jobs вынести worker в отдельный бинарник, сохранив общие parser/use_cases/repository modules и единый origin budget. API и UI не должны знать, где физически работает сборщик.
5. AI classification/embeddings worker и Qdrant — после baseline. Добавлять DTO, таблицы и библиотеки по реальной функциональности, не оставлять сейчас пустые сервисы.
6. Статистику расширять именованными полями/блоками с точными определениями; общий экран и endpoint уже отделены от каталога.

## Проверено и не проверено

Unit-тесты парсеров/robots/allowlist и HTTP-контракта, TypeScript/build/frontend tests; SQL integration на отдельном PostgreSQL: миграции, upsert, raw, pagination, shared lock, pause, interrupted recovery, общий rollback/commit и реальные HTTP handlers каталога/статистики/resume. Тесты не делают live requests.

2026-10-17: robots Internals получен (200, обычные темы/списки разрешены для нашей UA-группы), RFC robots вернул 404. Проверены HTML главной Internals, посты темы и rel=next с ?page=2 у длинной темы, iframe toc.html у RFC. Это точечные live-проверки, а не сквозной live-прогон сборщика. Нет собранного корпуса, проверки 5 000 тем, полного UI browser QA и нагрузочных измерений. Локальные проверки не разрешают массовый обход автоматически.


## Поиск B0 (реализовано 2026-10-24)

Модуль `src/search/` (analyzer, index, query) — проекция `documents` в Tantivy 0.27, не часть парсера. Одно поколение индекса = каталог `gen-<ms>` + `manifest.json` (версия анализатора, число документов); указатель `CURRENT` меняется атомарным rename, хранятся текущее и одно предыдущее поколение, индекс с другой версией анализатора не открывается. Пересборка полная, из repeatable-read снимка БД, один builder за раз (409). Outbox не нужен на этом этапе. Поля: `doc_id` (`source:native_id`), `db_id`, `source`, `url`, `completeness`, `title`/`body` (позиции, stored). Анализатор `rust-v1`: слова + `? ! ' :: -> => ..`, lowercase, без стоп-слов и стемминга. Запрос: OR по терминам (title ×3), фраза по позициям (title ×4, body ×2), точный ID ×100, фильтры source/complete_only без влияния на score. API: `GET /api/search`, `POST /api/search/rebuild`; сниппет — части `{text, matched}` без HTML. Тест postings (термин → документы, частоты, позиции) в `src/search/tests.rs`. Следующее: queries/qrels и отчёт по B0 (P3), затем typo/synonym/lemma по одному.

## Оценка B0 (P3, сделано 2026-11-05)

`src/evaluation/` (метрики nDCG@10, P@10, strict P@10, MRR@10, pooled recall, покрытие оценками, paired bootstrap по группам) и `src/bin/eval.rs` (`rebuild`, `run`, `pool`, `qrels`, `score`, `compare`, `review`, `agreement`). Данные и протокол — `eval/` (см. `eval/README.md`): 84 запроса, пул ~2000 кандидатов, 2066 суждений. **Оценки llm-silver** (модель, по названию и началу текста, вслепую к рангам); человеческий holdout (`eval review` → `eval/review/human.tsv` → `eval agreement`) ещё не выполнен. Заголовок RFC в индексе чистится (`RFC 1598: generic associated types`), анализатор `rust-v2`. Базовые цифры B0: `eval/reports/b0.md`.
