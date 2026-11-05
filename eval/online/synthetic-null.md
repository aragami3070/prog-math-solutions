# Online evaluation: hybrid-vs-b0 v1

Data: SYNTHETIC traffic from null.jsonl: checks the pipeline, says nothing about the search

- control `b0`: 2998 sessions; treatment `hybrid`: 3002 sessions; events of unknown variants ignored: 0
- SRM chi² = 0.003, p = 0.9588 (split matches the plan)
- sessions per arm needed for the planned effect: 1161
- p95 latency: control 58 ms, treatment 118 ms

## Verdict

No difference detected (2998 sessions in the smaller arm, 1161 needed to detect 5 points): the interval [-0.034, +0.011] includes zero, so an effect of 5 points or more is unlikely.

## Metrics (unit: session; 95% bootstrap interval of treatment − control)

| metric | role | control | treatment | difference | interval | p | p adj. |
|---|---|---|---|---|---|---|---|
| session success rate | primary | 0.729 | 0.718 | -0.012 | [-0.034, +0.011] | 0.314 | — |
| MRR of the first click | secondary | 0.459 | 0.447 | -0.012 | [-0.030, +0.008] | 0.243 | 1.000 |
| CTR@3 | secondary | 0.573 | 0.557 | -0.016 | [-0.038, +0.006] | 0.156 | 0.779 |
| CTR@10 | secondary | 0.631 | 0.620 | -0.011 | [-0.033, +0.010] | 0.297 | 1.000 |
| abandonment (queries without a click) | secondary | 0.369 | 0.380 | +0.011 | [-0.010, +0.033] | 0.297 | 1.000 |
| reformulation rate | secondary | 0.177 | 0.186 | +0.009 | [-0.004, +0.021] | 0.178 | 0.891 |
| zero-result rate | guardrail | 0.014 | 0.022 | +0.008 | [+0.003, +0.014] | 0.004 | — |
| degraded answers | guardrail | 0.000 | 0.000 | +0.000 | [+0.000, +0.000] | 1.000 | — |

Secondary p values are Bonferroni-adjusted; the primary metric is not. Guardrails are watched, not tested.

## Slices (first query of the session; reported with at least 100 sessions per arm)

| slice | control | treatment | success difference | interval |
|---|---|---|---|---|
| rfc_id | 368 | 376 | -0.037 | [-0.101, +0.027] |
| identifier | 367 | 384 | -0.032 | [-0.096, +0.035] |
| english_prose | 1881 | 1892 | +0.002 | [-0.026, +0.030] |
| russian | 382 | 350 | -0.033 | [-0.097, +0.033] |
