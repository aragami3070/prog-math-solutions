# Latency of the search modes

Measured 2026-11-05 on the development machine (1 CPU, 2 GB RAM, everything on one host: API,
PostgreSQL, Qdrant, embedding worker), release build, 4943 documents, index generation
`gen-1793879281909`. Each mode runs the 84 evaluation queries once after one warm-up request,
sequentially (no concurrency), `page_size=20`, wall time of the HTTP call from the client.

| mode | median ms | p95 ms | max ms | degraded answers |
|---|---|---|---|---|
| b0 (lexical) | 32 | 53 | 101 | 0 of 84 |
| lexical + typo/synonyms/lemmas | 34 | 56 | 111 | 0 of 84 |
| vector | 67 | 100 | 144 | 0 of 84 |
| hybrid | 212 | 352 | 482 | 0 of 84 |
| hybrid + typo/synonyms/lemmas | 196 | 284 | 368 | 0 of 84 |

Memory at the end of the run (resident): API 66 MB, embedding worker 796 MB, Qdrant 69 MB.

Limits of this measurement: one client, no parallel load, a warm index; 84 queries are enough
for medians but p95 and max rest on a handful of requests. The debug build is 10-15 times slower
(lexical median 406 ms, hybrid 2170 ms) and must not be used for latency claims. Hybrid costs
about 6 times the lexical answer because every query is embedded by the worker and both sides
return a pool of 100. Not measured here: restoring the database from a dump, load under
concurrent requests, a corpus larger than the current one.
