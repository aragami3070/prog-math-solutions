# Restore drill

`scripts/restore-check.sh`, одна машина (1 CPU, 2 ГБ RAM), debug-сборка `eval` (индекс в release строится быстрее).

Restore drill (2026-11-05T13:14:29Z)
- dump: 21.4 s, 87M; pg_restore into a scratch database: 14.9 s
- tables compared (documents, posts, links, edges, crawl_queue; count + content fingerprint): identical
- index rebuilt from the restored database: 113.2 s
- top results of 77 evaluation queries, live index vs rebuilt index (document and rank): identical

Что проверено: БД (дамп → чистая база → счётчики и отпечатки содержимого пяти таблиц), производные данные (индекс пересобран только из восстановленной БД), поведение (выдача по 77 запросам оценки совпала с живым индексом по документам и рангам).

Что нашла проверка: первый прогон показал расхождение, и оно оказалось недетерминизмом сборки, а не потерей данных. Две сборки одного корпуса различались порядком документов с равным баллом (позиции 13 и ниже, вне топ-10) из-за двух потоков записи индекса. Запись переведена в один поток; две сборки подряд теперь дают одинаковые выдачи по всем 84 запросам, nDCG@10 B0 не изменился (0.637).

Не проверено: восстановление Qdrant (коллекцию строит `embed_corpus.py`, около 70 минут на этой машине) и сырого HTML (в базе хранится извлечённый текст), restore на второй машине, rolling update.
