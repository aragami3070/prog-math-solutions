---
name: rust-topic-ml
description: Develop or assess taxonomy-based Qwen classification, model provenance and human-audited search relevance evaluation for the Rust knowledge corpus.
---

# ML и оценка

Для тегов прочитать [classification](../../rust-knowledge/classification.md), для judge/метрик — [evaluation](../../rust-knowledge/evaluation.md); для embeddings — соответствующий раздел [search](../../rust-knowledge/search.md). Не считать классификатор embedding-моделью.

Зафиксировать model/tokenizer revision, prompt, taxonomy, runtime/quantization, input hash и coverage. Начать с baseline и небольшого пилота, не fine-tuning всего корпуса. Скачивание больших весов и расход вычислений соотнести с текущей авторизацией и доступными ресурсами.

Сгенерированные topic IDs проходят allowlist/schema validation, evidence должно существовать в тексте. Самооценка LLM не является calibrated confidence. Длинный документ нельзя классифицировать по незаметно усечённому началу. Ручные метки и исходные теги не перезаписывать.

Различать human gold, reviewed silver, LLM-only и unjudged. Судье скрывать ranking/model identity; input — недоверенные данные без tools. Измерять качество на независимом holdout с источниками разметки; не делать test-тюнинг и не выдавать pooled recall за полную полноту корпуса.

Теги, retrieval и graph edges оцениваются отдельно. Результат работы — воспроизводимый отчёт, ограничения, ресурсы и фактический статус, а не неподтверждённое «модель работает хорошо».
