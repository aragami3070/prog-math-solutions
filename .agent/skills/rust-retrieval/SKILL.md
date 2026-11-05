---
name: rust-retrieval
description: Build or evaluate this project's Tantivy inverted index and lexical, lemma, fuzzy, synonym and Qdrant hybrid retrieval exposed through the Rust API.
---

# Поиск по Rust-документам

Прочитать [search](../../rust-knowledge/search.md), для контрактов [system](../../rust-knowledge/system.md), для сравнений [evaluation](../../rust-knowledge/evaluation.md). Основной интерфейс — API и сайт; отдельный CLI не является требованием.

Точные RFC IDs/Rust identifiers не портить prose-нормализацией. Сохранять positions и демонстрацию postings через тест или защищённую диагностику. Tantivy — библиотека: один writer на индекс, immutable snapshots для replicas; не имитировать встроенную распределённость.

Лемматизация и stemming различаются. Документы/запросы используют одинаковую NLP-версию, исходный prose и code остаются. Запрос не исправляется без обозначения original/effective query.

Фильтры совпадают в lexical/vector ветках. Vector chunks группировать до document ranking, scores объединять RRF, не суммой BM25/cosine. Проверять stale/deleted documents, generation и scope facets/candidates.

Сравнить с замороженным B0 и отдельными ablations. Test qrels не использовать для настройки. При отказе NLP/Qdrant возвращать явно отмеченный B0, не выдавать fallback за hybrid. Проверять API посредством тестов/curl/OpenAPI, без обязательного поискового CLI.
