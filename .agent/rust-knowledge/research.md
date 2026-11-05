# Первичные источники и исследовательские ограничения

Дата актуализации: 2026-10-17. Зависимости первого среза установлены и проверены сборкой/тестами; модели не скачивались, массовый сбор и нагрузочные замеры не проводились.

## Проверка первого среза, 2026-10-16/17

- Прямой HTTP к [Internals robots](https://internals.rust-lang.org/robots.txt) и примеру темы завершался timeout (20 секунд), а не 200. Старое наблюдение 2026-10-10 ниже не считается текущим разрешением. Transport в коде прекращает такой запуск до загрузки документов.
- [RFC robots](https://rust-lang.github.io/robots.txt) снова вернул 404; [оглавление RFC](https://rust-lang.github.io/rfcs/toc.html) было доступно. Это не проверка целого корпуса.
- [Internals terms](https://internals.rust-lang.org/tos) перечитаны; полный per-post license provenance ещё предстоит добавить. HTML-parser Internals проверен синтетическим fixture, текущий сырой live HTML из этой среды получить не удалось.
- [Vitest guide](https://vitest.dev/guide/) проверен при обновлении тестового инструмента: выбран Vitest 5, Node ≥22.12. Уязвимые первоначальные test dependencies обновлены; текущие разрешённые версии закреплены в `web/package-lock.json`.

## Источники корпуса

- [Rust Internals about](https://internals.rust-lang.org/about): обсуждения проектирования и реализации Rust.
- [Internals robots](https://internals.rust-lang.org/robots.txt): прямой HTTP-запрос 2026-10-10 вернул 200, группа `*` без общего запрета `/t/`; запрещены `/search`, auth/admin и иные служебные пути. Есть отдельный полный запрет для `rustbot` и некоторых других агентов; не подменять идентичность.
- [Internals terms](https://internals.rust-lang.org/tos): лицензии публикаций различаются по дате 2020-07-17; сохранять provenance на уровне постов.
- [RFC robots](https://rust-lang.github.io/robots.txt): прямой HTTP-запрос 2026-10-10 вернул 404. Не считать это лицензией или обещанием отсутствия rate limits.
- [RFC Book](https://rust-lang.github.io/rfcs/) и [пример Try trait](https://rust-lang.github.io/rfcs/3058-try-trait-v2.html): структурированные предложения, не справочник текущего stable API.

Разведка **2026-09-28**, не повторный подсчёт на 2026-10-10: 642 Markdown-файла в [text/](https://github.com/rust-lang/rfcs/tree/master/text); 8 514 уникальных URL в [sitemap_1.xml Internals](https://internals.rust-lang.org/sitemap_1.xml). Это не число успешно собранных полных HTML-документов.

## Технологии

- [Tantivy, официальный репозиторий](https://github.com/quickwit-oss/tantivy): embedded full-text библиотека на Rust, Lucene-подобный дизайн, BM25, configurable tokenizer, positions/facets. Не готовый распределённый сервер.
- [FuzzyTermQuery](https://docs.rs/tantivy/latest/tantivy/query/struct.FuzzyTermQuery.html): API нечёткого сопоставления терма; полноценные suggestions и UX исправлений проектируются отдельно. `latest` здесь ссылка на документацию, не рекомендация незакреплённой зависимости.
- [spaCy lemmatizer](https://spacy.io/api/lemmatizer): отдельный компонент лемматизации; rule-mode нуждается в предыдущем POS assignment. Это основание использовать готовый English pipeline в изолированном NLP-worker.
- [Qwen3-4B-Instruct-2507 model card](https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507): кандидат классификатора от Qwen. Финальные revision, лицензия весов, runtime и quantization проверяются до скачивания; нет обещания точности на RFC.
- [multilingual-e5-small model card](https://huggingface.co/intfloat/multilingual-e5-small): кандидат embedding-модели. HTML-страница превысила лимит web-чтения; raw README был доступен, но фиксированная конфигурация ещё не выбрана. Перед запуском прочитать нужные поля и закрепить revision.
- [Sentence Transformers](https://www.sbert.net/examples/sentence_transformer/applications/computing-embeddings/README.html): шаблоны query/passage зависят от модели, длинный input может усекаться. Поэтому нужен контролируемый chunking с tokenizer и coverage.
- [Cytoscape.js](https://js.cytoscape.org/): визуализация и взаимодействие с графом, не хранилище фактов.
- [utoipa](https://github.com/juhaku/utoipa): OpenAPI для Rust и интеграция Swagger UI; проверить совместимость выбранных crate versions с Axum.

Ресурсы в platform.md — инженерные оценки. Сам факт чтения model card не означает запущенный inference. Прямой запрет или отказ источника нельзя исправлять сменой IP/маскировкой User-Agent; ограничения отражаются в отчёте P0.
