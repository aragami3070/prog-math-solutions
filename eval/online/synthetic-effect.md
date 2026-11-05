# Online evaluation: hybrid-vs-b0 v1

Data: SYNTHETIC traffic from effect.jsonl: checks the pipeline, says nothing about the search

- control `b0`: 2954 sessions; treatment `hybrid`: 3046 sessions; events of unknown variants ignored: 0
- SRM chi² = 1.411, p = 0.2349 (split matches the plan)
- sessions per arm needed for the planned effect: 1147
- p95 latency: control 58 ms, treatment 118 ms

## Verdict

hybrid is better on the primary metric (2954 sessions in the smaller arm, 1147 needed to detect 5 points): +0.054 [+0.033, +0.074].

## Metrics (unit: session; 95% bootstrap interval of treatment − control)

| metric | role | control | treatment | difference | interval | p | p adj. |
|---|---|---|---|---|---|---|---|
| session success rate | primary | 0.734 | 0.787 | +0.054 | [+0.033, +0.074] | 0.000 | — |
| MRR of the first click | secondary | 0.481 | 0.498 | +0.017 | [-0.002, +0.036] | 0.085 | 0.427 |
| CTR@3 | secondary | 0.591 | 0.627 | +0.036 | [+0.015, +0.057] | 0.001 | 0.007 |
| CTR@10 | secondary | 0.644 | 0.695 | +0.051 | [+0.030, +0.071] | 0.000 | 0.000 |
| abandonment (queries without a click) | secondary | 0.356 | 0.305 | -0.051 | [-0.071, -0.030] | 0.000 | 0.000 |
| reformulation rate | secondary | 0.175 | 0.166 | -0.009 | [-0.022, +0.003] | 0.153 | 0.766 |
| zero-result rate | guardrail | 0.015 | 0.014 | -0.002 | [-0.007, +0.003] | 0.450 | — |
| degraded answers | guardrail | 0.000 | 0.000 | +0.000 | [+0.000, +0.000] | 1.000 | — |

Secondary p values are Bonferroni-adjusted; the primary metric is not. Guardrails are watched, not tested.

## Slices (first query of the session; reported with at least 100 sessions per arm)

| slice | control | treatment | success difference | interval |
|---|---|---|---|---|
| rfc_id | 357 | 373 | +0.031 | [-0.030, +0.091] |
| identifier | 366 | 396 | +0.030 | [-0.033, +0.091] |
| english_prose | 1877 | 1932 | +0.053 | [+0.025, +0.080] |
| russian | 354 | 345 | +0.111 | [+0.047, +0.174] |

## Agreement with the offline result

Offline: hybrid vs B0 nDCG@10 +0.078 (95% CI +0.036..+0.120). Online primary metric: +0.054. Clicks are biased by position and snippet; a higher click rate is not the same as higher relevance, and a disagreement is reported as found.
