# Сайт и API

Текущий срез 2026-10-17: `/` — каталог с пагинацией, `/stats` — статистика, API/Swagger для ручного ограниченного сбора. Подробности и реальные endpoints: [current-slice](current-slice.md). Экраны/контракты ниже — дальнейший план, не текущая реализация. Не добавлять заглушки поиска/графа/админки на существующие два экрана.

Уточнение пользователя: в текущем локальном срезе авторизации нет, сервер принимает только loopback-адрес в `HTTP_BIND`. Требования к защищённым административным операциям ниже относятся к будущему внешнему развёртыванию.

## Экраны

- `/search`: поисковая строка, режим, переключатели улучшений, фильтры source/date/source tags/AI topics; результаты со snippets, датой, источником и маркировкой машинных меток. URL хранит query/filters/mode/page; back/forward восстанавливают выдачу.
- `/documents/:id`: заголовок, авторство, даты, темы и их provenance, фрагменты/разделы и исходная ссылка. Не смешивать цитату участника с официальным решением. Код выделяется безопасным renderer.
- `/topics/:id`: описание темы и документы; видны непроверенные машинные метки, при необходимости режим reviewed-only.
- `/graph?document=...`: локальный граф и доступный список соседей по [graph](graph.md).
- `/evaluate`: запрос, инструкция о релевантности, независимая разметка 0/1/2/skip с evidence; gated admin/reviewer, не публичная неограниченная запись.
- `/admin`: crawl/index/NLP jobs, counts и ошибки, пауза/возобновление, версии corpus/index/model. Запуск операций требует авторизации, ресурсы и лимиты видимы.

Не имитировать готовый сбор демоданными без маркировки. Mock mode не проходит live-приёмку. Сайт не является редактором заметок: backlinks и граф не требуют копировать весь функционал Obsidian/Anytype.

## Контракты

- `GET /api/search?q=&mode=&source=&topic=&from=&to=&cursor=&limit=`: original/effective query, mode/flags, degradation, generation, facets scope, results и next cursor.
- `GET /api/documents/{id}`: документ и provenance; версии AI/topics отдельно.
- `GET /api/topics` и `GET /api/topics/{id}/documents`: taxonomy version, descriptions, filters.
- `GET /api/graph?document_id=&depth=&edge_type=&limit=`: bounded graph, evidence, generation, truncated.
- `POST /api/evaluations`: query ID, document ID, grade, evidence, assessment version; сервер валидирует доступ и принадлежность набору.
- `/api/admin/jobs`, `/api/admin/crawl/pause`, `/api/admin/crawl/resume`, `/api/admin/reindex`: защищённые операции с audit и dedup/idempotency key.

OpenAPI — обязательный контракт, Swagger UI — dev-интерфейс проверки; примеры curl и автоматические API contract tests обязательны. Отдельный CLI не требуется.

Input limits: query ≤512 символов, limit ≤50, server-bound graph/candidate/deadline limits. Это стартовые значения, должны совпадать в frontend/API. Не принимать arbitrary URL для fetch или произвольный query DSL; graph/source IDs валидируются.

## Состояние и качество

TanStack Query владеет server cache; Zustand — выбранный узел, панели, UI preferences, не копия результатов. AbortController и query keys предотвращают перезапись новой выдачи поздним старым ответом.

Loading, empty, error, degraded и index-updating — разные состояния. Не отключать фильтры при пустой выдаче без согласия. Отсутствие Qwen не ломает просмотр уже сохранённых тегов. Отсутствие Qdrant не скрывать за ярлыком hybrid.

HTML очищается или преобразуется в безопасные компоненты; внешние ссылки безопасно открываются, никакие scripts/event handlers из форума не исполняются. Админ-токены не попадают в frontend bundle, CORS не `*` для приватных записей. Клавиатура, фокус, читаемость и мобильная ширина проверяются отдельно; граф имеет текстовый эквивалент.
