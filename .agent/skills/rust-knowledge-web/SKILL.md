---
name: rust-knowledge-web
description: Implement or review this project's Axum OpenAPI backend, React search and topic pages, provenance-aware graph and local deployment.
---

# Сайт поиска и графа

Прочитать [web](../../rust-knowledge/web.md) и [system](../../rust-knowledge/system.md); для графа — [graph](../../rust-knowledge/graph.md), для VM/Compose/Kubernetes — [platform](../../rust-knowledge/platform.md).

Основной путь проверки — API contract tests, curl/OpenAPI/Swagger и сайт. Не вводить обязательный CLI и не заменять Rust backend платформенным starter другого языка. Внешняя публикация отдельно от локальной реализации.

URL хранит запрос/фильтры/режим, TanStack Query — server cache, Zustand — локальный UI. Loading/empty/error/degraded различаются. Поздний ответ не перезаписывает новый запрос. Оригинальные тексты безопасно рендерятся без выполнения HTML/scripts.

В графе references, has_topic и similar_to имеют разные provenance и стили. Сходство не объявлять фактической цитатой/влиянием; bounded subgraph с truncated flag и текстовой альтернативой, не весь корпус в браузер.

Защищать admin jobs/диагностику/разметку; API не fetch-ит произвольные пользовательские URLs. Обновлять OpenAPI вместе с handlers. Проверять изменённые сценарии, клавиатуру/мобильную ширину и ошибки зависимостей; при невозможности browser QA явно сообщать предел проверки.

Для масштабирования сохранять одного writer на Tantivy snapshot и общий crawl budget. Нагрузочные тесты только своей системы. Не обещать HA на нескольких VM одного хоста.
