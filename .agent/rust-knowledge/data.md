# Корпус, экспорт и версии

Текущий срез: raw HTML + актуальные normalized documents хранятся в PostgreSQL. Реализован экспорт v1 через `GET /api/sources/export?format=jsonl|json|csv|zip`, см. [README](../../README.md). JSON API каталога по-прежнему содержит только excerpt. Экспорт сохраняет доступный provenance и явно отмечает его пробелы; история normalized versions ещё не реализована.

## Реализованный контракт v1

Параметры: `format` (default jsonl), `source` (не задан / internals / rfc), `complete_only` (default false), `spreadsheet_safe` (default false, влияет только на CSV). Ошибки: 400 — параметры, 409 — другая подготовка экспорта в этом процессе, 503 — БД, 500 — сериализация/временное хранилище. Формирование завершается до HTTP 200; Content-Disposition задаёт имя файла, Cache-Control — no-store.

Одна repeatable-read read-only транзакция на подготовку; после формирования файл отдаётся без удержания транзакции. Порции по 32 документа с keyset по document_id и C collation. Промежуточные файлы через tempfile, удаляются при закрытии; ZIP без сжатия. Один документ может быть большим: ограничение памяти — порция документов, а не строго фиксированное число байтов. Нужен запас места в системном временном каталоге, особенно для ZIP.

JSON — массив, JSONL — те же объекты по одному на строку. Пустой корпус: `[]` в JSON, пустой JSONL, только заголовок CSV, ZIP с четырьмя валидными файлами. CSV содержит все поля записи; массивы/объекты — JSON в ячейках, null — пустая ячейка. `spreadsheet_safe` добавляет апостроф для потенциальных формул в строковых ячейках и намеренно меняет CSV-проекцию; канонический JSON не меняется.

ZIP: corpus.jsonl, corpus.json, corpus.csv, manifest.json. В manifest: schema_version, snapshot_at, corpus_sha256 (SHA-256 JSONL), описание хеша, filters, spreadsheet_safe, documents, documents_by_source, documents_by_completeness, referenced_fetches, parser_versions, files (bytes/sha256), raw_html_included=false, limitations. Counts относятся к выбранным документам; referenced_fetches — уникальные известные fetch IDs, **не** доказательство числа полных тем/постов/всех страниц сайта. Независимые запросы могут дать разные снимки при активном сборе; для согласованных форматов использовать один ZIP.

Заголовки каждого скачивания: X-Corpus-Schema-Version=1, X-Corpus-Documents, X-Corpus-Snapshot-At, X-Content-SHA256 (хеш скачиваемого файла, для ZIP — архива). Manifest хранится только внутри ZIP, постоянного сервера артефактов нет.

Текущие пробелы provenance: language/source_category/updated_at — null, source_tags — []; authors и published_at берутся только из известных постов (дата темы только из post_1), fetched_at — реальное время связанного fetch. License ID не угадан: license_provenance содержит source_url, license_id=null, status=unresolved. Sections строятся из `posts`: post ID `post_N` у Internals, `section_1` у RFC; heading может быть null. Ссылки берутся из `links` и содержат target_url, section_id поста и anchor_text. `metadata` экспорта — license_source и список страниц (page_no, url, fetch_id). Для RFC пока один блок main, не разбиение по каждому заголовку. Это ограничения качества данных, а не основания подставлять вымышленные значения.

Дополнительные поля экспортируемой записи: first_seen_at, last_seen_at, metadata. JSON/JSONL позволяют строить поиск офлайн без raw HTML. Условие 5 000 полных тем, полная классификация лицензий и история версий остаются отдельными задачами.

## Идентичность и нормализованная запись

Одна запись `document` — тема Internals или текст RFC. ID — `internals:<topic_id>` / `rfc:<number>`, не номер загрузки или позиция в индексе. Посты/разделы и chunks имеют собственные стабильные ключи и parent document ID.

Обязательные поля canonical JSONL v1:

- `schema_version`, `document_id`, `source`, `native_id`, `canonical_url`, `title`, `language`;
- `published_at` (nullable при отсутствии), `updated_at` (nullable), `fetched_at` — UTC ISO 8601; fetch-time не подменяет дату публикации;
- `authors`, `source_category`, `source_tags` — исходные метаданные без догадок;
- `sections` — упорядоченные блоки с `section_id`, `post_id` при наличии, author/date/license, `heading`, `text`, `code_blocks`, `source_url`;
- `body_text`, `links` (`target_url`, `section_id`, anchor text), `content_hash`;
- `completeness`: `complete|partial|unknown` + причина; `fetch_ids`, `parser_version`, `license_provenance`.

Производные артефакты отдельно: `lemmas.jsonl`, `classifications.jsonl`, `chunks.jsonl`, `edges.jsonl`. Каждый ссылается на document ID + content hash + pipeline/model/taxonomy revision. Исходный текст не перезаписывается результатом LLM.

## Форматы

- JSONL — канонический поток: UTF-8, один JSON object на строку, без комментариев, пригодный к потоковому чтению.
- JSON — массив тех же объектов для буквального требования JSON и простого просмотра; выдаётся отдельной командой.
- CSV — одна строка на документ: ID, source, URL, title, dates, language, body, completeness, content hash; списки tags/authors сериализуются JSON-массивом внутри корректно quoted ячейки. Переводы строк и кавычки экранируются библиотекой, не заменяются вручную.
- CSV — удобная плоская проекция, не замена вложенному provenance. Отдельный spreadsheet-safe экспорт нейтрализует formula-like значения; не портить канонический текст ради Excel.
- `manifest.json`: schema, corpus version/hash, дата, источники, counts по документам/темам/RFC/pages/chunks, исключения, parser version и checksums файлов. Секреты, cookies и auth headers не экспортировать.

Строки сортируются детерминированно по ID при финальной выгрузке; неизвестные значения — null в JSON и согласованный empty в CSV. Стабильность даты экспорта не требуется, стабильность содержимого при одинаковом input/parser — требуется.

## Проверки и счётчики

Validate: уникальность ID, обязательные поля, canonical в allowlist, валидные даты, сопоставление sections/fetches, schema version, отсутствие меню вместо body. Экспорт JSON и JSONL должен восстановить одинаковые нормализованные объекты; CSV проверяется с учётом документированной потери вложенной структуры.

Индекс можно перестроить из экспорта без сети; формат запуска (job, тест, служебная команда) не является требованием домашки. Fixtures маркируются synthetic и не входят в 5 000 live-тем. Отчёт разделяет загрузки, страницы продолжения, темы, RFC, posts и chunks. Тема с неизвестной полнотой не проходит строгую приёмку.

Обновление: сохранять новую версию raw/normalized; invalidation всех старых производных результатов по content hash. Удаление: tombstone и удаление из обеих поисковых проекций, classifications и edges; retries старых задач не должны воскресить удалённый документ. Смена schema/parser требует явной миграции или rebuild.
