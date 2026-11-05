# Ресурсы и масштабирование

Все мощности — стартовые оценки для пилота, не измеренные минимумы и не обещания throughput.

| Стадия | Стартовый ориентир |
|---|---|
| HTML → экспорт → Tantivy + минимальный API | 2–4 vCPU, 4–8 ГБ RAM, 20–40 ГБ SSD; index-writer budget ограничить |
| Сайт + PostgreSQL + Tantivy + Qdrant, без Qwen | 4 vCPU, 8–16 ГБ RAM, 60–100 ГБ SSD |
| Локальная Qwen-классификация | Отдельный процесс/узел; выбор после замера GPU/CPU/RAM и длины входа |

Для модели 4B только веса FP16 порядка 8 ГБ (4 млрд × 2 байта); 4-bit — около 2 ГБ теоретических packed weights. Runtime, quantization metadata, KV cache и batch требуют дополнительной памяти. Это не минимальная VRAM и не гарантия размещения. Начать с короткого batch и bounded context; не держать embedding и генеративную модель одновременно без замеров. Поиск не ждёт полного прогона классификатора.

Диск = raw HTML × сохранённые версии + exports + БД/WAL + Tantivy + Qdrant + model cache + логи + временный rebuild. Пример векторов: 100 000 × 384 × 4 ≈154 МБ только сырых float32, без HNSW/payload. Реальные dimension/count берутся из выбранного embedding pipeline. Нужны запас под двойной индекс и 30% свободного места; backup отдельно.

## Compose и восстановление

Первый вертикальный срез — API/сайт, проверяемые через curl и OpenAPI. Compose: API, web, PostgreSQL; Qdrant и NLP-worker включаются на соответствующих этапах. Внешние источники доступны только fetcher. Raw/exports/indices на явных volumes, секреты вне репозитория. Живые индексы и модели не хранить в Git.

Сохранённые raw/exports/schema/config/таксономия/ручные метки позволяют восстановить систему без повторного скрапинга. Проверять restore: БД, raw/exports, классификация и qrels; производные индексы можно перестроить. Отчёт о восстановлении и фактическом времени обязателен до multi-VM.

## VM и Tantivy

Сначала одна VM. Далее разделить app/search, data и NLP по измеренному bottleneck. Tantivy — библиотека, не распределённый кластер. Kubernetes сам по себе не превращает её в распределённый движок.

Для 5–10 тысяч документов шардирование не нужно. Несколько API/search replicas получают **immutable snapshot** одной generation на локальный диск; один indexer строит новую generation, публикует manifest/checksum, readers проверяют и атомарно переключаются. Не писать общий индекс несколькими pod и не использовать NFS как предположительно безопасную замену replication.

Reverse proxy балансирует stateless API; readiness учитывает загруженную generation. Cursor привязан к snapshot; либо общий небольшой result cache, либо routing к совместимой generation, либо явный restart выдачи. Во время rolling update временно не смешивать несогласованные поколения Tantivy/Qdrant.

Qdrant/PostgreSQL сначала одиночные: разделение VM не означает HA. Репликация и независимые failure domains — отдельный этап. Сотни реплик и терабайты не нужны ради демонстрации Kubernetes.

Workers масштабируются по локальной очереди CPU/ML задач. Внешний crawl budget един на домен и не растёт с числом реплик; отказ общего limiter закрывает egress. Нагрузочные тесты направлены только на своё API и mock origins.

## Kubernetes/K3s позднее

Deployment для API/worker, Ingress/Service, requests/limits, readiness/liveness, graceful shutdown, Secrets и PVC; HPA для API/парсинга, не для увеличения внешних запросов. Индексация/model inference имеют bounded concurrency. Мониторить latency, error rate, queue age, source pauses, index generation, corpus coverage, RAM/VRAM/disk. Цель p95≤1 секунды при 5 req/s без генерации — гипотеза для замера, не SLA.
