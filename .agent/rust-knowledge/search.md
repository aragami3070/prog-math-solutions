# Обратный индекс и поиск

## Базовый индекс Tantivy

Один документ Tantivy соответствует одному исходному документу. Поля: stored/exact document ID и canonical URL; title/body с токенами, частотами и позициями; exact Rust identifiers и RFC number; source, dates, source tags и AI topics для фильтров; stored text для snippets. В lemma-режиме добавляются отдельные title/body lemma fields; оригинальные остаются.

Обратный индекс связывает term с posting list: internal document address, term frequency и позициями. Наружу всегда переводить в стабильный document ID: segment/doc addresses меняются после merge. Интеграционный тест или dev/admin inspect endpoint демонстрирует словарь/postings и подтверждает критерий 4.

Токенизацию проверить на `Try`, `FromResidual`, `async fn`, `?`, `GAT`, `Send`, `!Send`, `3058`: punctuation-sensitive запросы не должны исчезать после обычного prose tokenizer. Номер RFC и точный identifier имеют приоритет над fuzzy. Частотные stopwords не удалять из кода и identifier fields. Phrase search использует позиции, не набор слов.

Baseline B0: title/body BM25 + exact boost + фильтры, без LLM, синонимов и исправлений. Зафиксировать corpus/index/analyzer versions до улучшений.

## Улучшения по отдельности

1. **Опечатки:** ограниченная fuzzy-ветка с edit distance и пониженным весом; для коротких слов, номеров и точных идентификаторов отключена по умолчанию. Correction suggestions из словаря корпуса; UI/API показывают original/effective query, не меняют запрос молча. Обычный fuzzy matching и полноценная коррекция запроса — разные функции.
2. **Синонимы/аббревиатуры:** курируемый versioned словарь (`GAT` ↔ `generic associated types`, `NLL` ↔ `non-lexical lifetimes`). Query-time expansion с меньшим boost, лимитом вариантов и сохранением phrase semantics. Связанные понятия не объявлять синонимами. LLM может предложить пары, но не публикует их без проверки.
3. **Лемматизация:** spaCy English pipeline с POS перед rule lemmatizer; преобразует только prose. Одинаковая версия обработки для документов и запроса; API вызывает локальный NLP-worker, а при его отказе явно возвращается к B0. Леммы индексируются отдельным analyzer без повторного stemming. Проверка: нерегулярные формы и контекстная неоднозначность; stemmer не считается выполнением этого пункта. Первое покрытие — английский корпус, русские queries относятся к cross-lingual retrieval.
4. **Векторы:** самостоятельная embedding-модель, первый кандидат `intfloat/multilingual-e5-small`. Версию/лицензию и лимиты подтвердить перед скачиванием; query/passage templates следуют model card. Chunks по разделам/постам и токенам фактического tokenizer, без тихого усечения длинного RFC; overlap и лимит выбираются пилотом. Qdrant хранит document ID, hash, chunk URL и metadata.
5. **Hybrid:** получить lexical top-K и vector top-K под одинаковыми фильтрами. Vector chunks сгруппировать до document rank (лучший chunk, ограничение вклада длинных тем), потом RRF: сумма `1/(k+rank)` для присутствующих веток. Не складывать raw BM25/cosine. Параметры K/k выбрать на dev, не на test.

Embeddings, классификатор и reranker не взаимозаменяемы. Qwen-классификация не является векторным поиском. Reranker и перевод запроса — поздние эксперименты; базовые критерии не зависят от их наличия.

## Фильтры, обновления и отказ

Жёсткие фильтры source/date/topic одинаковы в Tantivy и Qdrant; hydration повторно отсеивает удалённые/устаревшие документы. Переключать согласованное generation только когда обе проекции готовы; версии возвращать в ответе. При сбое vector/NLP режим помечается degraded, обычный B0 продолжает работать.

`total`/facets для полного lexical индекса и размер ограниченного hybrid candidate pool — разные величины. В API возвращать scope. Pagination cursor привязан к generation, query, filters и mode; при смене generation — явный reset, не тихие дубли. На MVP ограничить глубину выдачи и размер candidate pool.

Новый индекс строится в отдельном каталоге; проверить counts/контрольные запросы и атомарно переключить manifest. Один writer владеет конкретным индексом; API readers перезагружаются после commit. Не открывать несколько writers на общем сетевом диске.

## Приёмка

Rebuild из JSONL без внешних сайтов; поиск через API и inspect через тест/dev endpoint. Повторное открытие индекса даёт те же результаты при фиксированных данных. Тесты exact/phrase/typo/synonym/lemma/filter, отсутствие дублей, удаления и смена generation. Каждое улучшение оценивается по [evaluation](evaluation.md), отрицательный результат фиксируется, не скрывается.
