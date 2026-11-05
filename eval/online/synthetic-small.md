# Online evaluation: hybrid-vs-b0 v1

Data: SYNTHETIC traffic from small.jsonl: checks the pipeline, says nothing about the search

- control `b0`: 186 sessions; treatment `hybrid`: 214 sessions; events of unknown variants ignored: 0
- SRM chi² = 1.960, p = 0.1615 (split matches the plan)
- sessions per arm needed for the planned effect: 1266
- p95 latency: control 58 ms, treatment 118 ms

## Verdict

Preliminary only (186 sessions in the smaller arm, 1266 needed to detect 5 points): the interval excludes zero (difference +0.148, [+0.064, +0.231]) but the sample is below the plan; repeat with more traffic before relying on it.

## Metrics (unit: session; 95% bootstrap interval of treatment − control)

| metric | role | control | treatment | difference | interval | p | p adj. |
|---|---|---|---|---|---|---|---|
| session success rate | primary | 0.694 | 0.841 | +0.148 | [+0.064, +0.231] | 0.000 | — |
| MRR of the first click | secondary | 0.449 | 0.559 | +0.109 | [+0.031, +0.185] | 0.005 | 0.023 |
| CTR@3 | secondary | 0.573 | 0.681 | +0.109 | [+0.023, +0.192] | 0.012 | 0.058 |
| CTR@10 | secondary | 0.621 | 0.719 | +0.098 | [+0.015, +0.177] | 0.018 | 0.090 |
| abandonment (queries without a click) | secondary | 0.379 | 0.281 | -0.098 | [-0.177, -0.015] | 0.018 | 0.090 |
| reformulation rate | secondary | 0.164 | 0.185 | +0.021 | [-0.028, +0.071] | 0.398 | 1.000 |
| zero-result rate | guardrail | 0.013 | 0.029 | +0.015 | [-0.007, +0.040] | 0.217 | — |
| degraded answers | guardrail | 0.000 | 0.000 | +0.000 | [+0.000, +0.000] | 1.000 | — |

Secondary p values are Bonferroni-adjusted; the primary metric is not. Guardrails are watched, not tested.

## Slices (first query of the session; reported with at least 100 sessions per arm)

| slice | control | treatment | success difference | interval |
|---|---|---|---|---|
| rfc_id | 21 | 32 | not enough sessions | — |
| identifier | 25 | 30 | not enough sessions | — |
| english_prose | 116 | 118 | +0.140 | [+0.029, +0.243] |
| russian | 24 | 34 | not enough sessions | — |
