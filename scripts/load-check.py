#!/usr/bin/env python3
"""Open-loop load check of the search API: requests are sent at a fixed rate whatever the
answers' speed, so a slow server shows queueing instead of hiding it.

    scripts/load-check.py --url http://127.0.0.1:8099 --rate 5 --seconds 60 [--mode hybrid]

Queries are the evaluation queries, in a fixed shuffled order. Prints median/p95/p99/max
latency, the error count and the achieved rate. Aim it only at your own server.
"""
import argparse
import json
import random
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("--url", default="http://127.0.0.1:8099")
parser.add_argument("--rate", type=float, default=5)
parser.add_argument("--seconds", type=int, default=60)
parser.add_argument("--mode", default="lexical")
args = parser.parse_args()

queries = [json.loads(line) for line in open("eval/queries.jsonl")]
random.Random(1).shuffle(queries)
latencies, errors = [], []
lock = threading.Lock()


def one(q):
    params = {"q": q["text"], "page_size": 20, "mode": args.mode}
    if q["filters"]["source"]:
        params["source"] = q["filters"]["source"]
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(f"{args.url}/api/search?" + urllib.parse.urlencode(params), timeout=30) as r:
            r.read()
        ms = (time.perf_counter() - started) * 1000
        with lock:
            latencies.append(ms)
    except Exception as error:  # noqa: BLE001 - any failure counts
        with lock:
            errors.append(repr(error)[:80])


threads, total = [], int(args.rate * args.seconds)
start = time.perf_counter()
for i in range(total):
    delay = start + i / args.rate - time.perf_counter()
    if delay > 0:
        time.sleep(delay)
    t = threading.Thread(target=one, args=(queries[i % len(queries)],))
    t.start()
    threads.append(t)
for t in threads:
    t.join()
elapsed = time.perf_counter() - start
latencies.sort()
pct = lambda p: latencies[min(len(latencies) - 1, int(p * len(latencies)))] if latencies else float("nan")
print(json.dumps({
    "mode": args.mode, "target_rate": args.rate, "requests": total, "ok": len(latencies), "errors": len(errors),
    "achieved_rate": round(total / elapsed, 2), "median_ms": round(pct(.5)), "p95_ms": round(pct(.95)),
    "p99_ms": round(pct(.99)), "max_ms": round(latencies[-1]) if latencies else None,
    "first_error": errors[0] if errors else None}))
