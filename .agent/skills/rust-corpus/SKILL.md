---
name: rust-corpus
description: Implement or review Rust Internals and RFC HTML ingestion, completeness checks, provenance and JSON/CSV exports for this project.
---

# Корпус Internals и RFC

Перед изменением прочитать [ingestion](../../rust-knowledge/ingestion.md) и [data](../../rust-knowledge/data.md). Для нового live-обхода проверить дату наблюдений в [research](../../rust-knowledge/research.md), актуальные robots и условия; сначала разрешённая малая выборка.

Тема, HTML-страница продолжения, post и chunk — разные счётчики. В минимум 5 000 включать только полные уникальные темы Internals. Если HTML отдаёт только начало длинной темы, не обозначать её complete; API/JSON не подменяет HTML-критерий автоматически.

Все requests, redirects и retries проходят общий per-origin budget. Workers не увеличивают внешний rate. 403/капча требуют остановки источника; повторные 429 — остановки запуска. Собственная честная идентичность, никаких обходов блокировки.

Сохранять raw HTML, URL, fetch time, parser version, hash и лицензии/авторство постов. JSONL — canonical; JSON и CSV — проверяемые exports. При повторном парсинге сеть не нужна. Изменение контракта отражать в data.md.

Проверять idempotency, полноту пагинации, код/цитаты, deleted/malformed документы, escaping и round-trip exports на fixtures/mock origins. CI не обходит сайты. В отчёте различать план, fixture-ready parser и реально проверенный live-корпус.
