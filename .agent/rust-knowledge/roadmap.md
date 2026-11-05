# Этапы реализации и приёмка

Уточнение 2026-10-10: сайт — основной интерфейс; API/curl/Swagger достаточны для проверки. Поисковая CLI необязательна и не является этапом, блокирующим веб-разработку.

Текущее состояние: один Rust package, PostgreSQL, HTML-обход с durable очередью/resume, каталог + статистика, OpenAPI и тесты. Реализован экспорт JSON/JSONL/CSV/ZIP с manifest, фильтрами и согласованным снимком БД. См. [current-slice](current-slice.md) и [data](data.md). Полнота 5 000 тем и поиск ещё не подтверждены/не реализованы. Следующий этап — Tantivy и страница поиска; онлайн-оценка (A/B, метрики) — последний этап, P10. Разделение на workspace crates не требуется; будущие модули не создавать заранее.

| Этап | Работа | Артефакт и gate |
|---|---|---|
| P0: feasibility | Перечитать robots/условия, проверить 100–200 тем + 20–30 RFC и продолжения длинных тем | Отчёт HTML completeness, правила и запас для 5 000 тем; без разрешённого полного HTML не переходить к массовому сбору |
| P1: вертикальный срез | Развитие текущего Rust package: модели, parser, JSONL/JSON/CSV, Tantivy, Axum и минимальная страница поиска | На малом корпусе сайт ищет реальные документы; API проверяется curl/OpenAPI/Swagger; rebuild без источников |
| P2: надёжный сбор | Очередь/resume, per-origin limits, retries, raw store, source adapters | ≥5 000 полных уникальных тем, RFC отдельно; manifest и ручная выборка 100 тем |
| P3: baseline | Схема обратного индекса, BM25/exact/filters/snippets, snapshot versions, тест postings | Зафиксированные B0, queries/qrels и исходный quality report |
| P4: lexical features | Typo suggestions, словарь синонимов, POS-aware лемматизация | Независимые режимы в API/UI, тесты и абляция каждого изменения |
| P5: semantic search | Embedding pipeline, Qdrant, фильтры, parent grouping, RRF | Vector/hybrid режимы и сравнение с B0 на holdout; явная деградация |
| P6: классификация | Taxonomy, исходные теги, baseline, Qwen pilot и batch pipeline | Метки с evidence/provenance, review flow, F1 и отчёт об ошибках/ресурсах |
| P7: граф | References + document-topic links, bounded UI | Разные типы связей, объяснения и доступный список; semantic edges только после отдельной оценки |
| P8: финальная оценка | Замороженный test, человеческий аудит LLM-разметки, latency/restore | qrels/runs/metrics/report, ограничения результатов, все семь критериев проверены |
| P9: масштабирование | Compose → VM → K3s, snapshots и балансировка | Restore/rolling update test, общий crawl budget сохранён; измеренные мощности |
| P10: онлайн-оценка | Логирование поисковых событий, A/B или interleaving B0 против лучшего варианта, поведенческие метрики и guardrails | Отчёт с планом анализа, размером выборки и честным выводом; оценка не заменяет offline из P8 |

P1 намеренно включает сайт рано, чтобы последующие возможности сразу можно было демонстрировать. P0/P2 могут выявить ограничения источников: fixtures помогают разрабатывать, но не заменяют требуемый реальный корпус.

## Проверки по зонам

- **Сбор:** fixtures короткой/длинной темы, пагинация, пропуски постов, цитаты, code blocks, malformed HTML, robots/redirects/429/403. CI использует mock origins и локальные fixtures, не обходит live-сайты.
- **Данные:** идентичность, версии, idempotency, round-trip JSON/JSONL и CSV escaping; счётчики загрузок/тем/RFC/chunks раздельны.
- **Поиск:** postings/positions; exact IDs, phrase, filters; typo/synonym/lemma; одинаковые predicates в lexical/vector; удаление и обновление без stale chunks.
- **API/UI:** OpenAPI соответствует handlers, curl examples работают; loading/empty/error/degraded; URL navigation, отмена старого запроса, авторизация admin, XSS из HTML, мобильная версия и клавиатура.
- **ML:** bounded JSON schema, taxonomy allowlist, evidence validation, abstention, ручные решения не затираются; отсутствие модели не ломает поиск.
- **Граф:** canonical links, отсутствие self/duplicate edges, тип/provenance, bounded depth/limits и обновления версий.
- **Качество:** [протокол оценки](evaluation.md), baseline и ablation, отдельно английские/русские запросы и rare topics; unjudged не выдаются за human negatives.
- **Онлайн-оценка:** события без персональных данных, стабильное назначение вариантов, SRM-проверка, guardrails (latency, доля пустых выдач, ошибки), план анализа до запуска; см. [evaluation](evaluation.md#онлайн-оценка-ab-тесты-и-поведенческие-метрики-p10).
- **Эксплуатация:** восстановление raw/exports/меток и перестройка проекций без внешнего crawl, один writer на Tantivy index, совместимые snapshots у replicas.

## Не считать выполненным

Красивый сайт на mock JSON не закрывает сбор и индекс. Наличие sitemap не подтверждает 5 000 пригодных документов. Рабочий endpoint без проверки relevance не закрывает качество. Наличие stemmer не закрывает lemmatizer. Qwen-теги не закрывают vector search. Картинка графа без provenance не закрывает модель связей.

Пока неизвестны характеристики машины для инференса и дополнительные требования преподавателя. Это не мешает P0/P1; выбор runtime/quantization и обязательность отдельных инфраструктурных технологий уточнять перед соответствующим этапом.
