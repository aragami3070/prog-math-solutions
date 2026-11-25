#!/usr/bin/env python3
"""Candidate `similar_to` pairs from the vectors already in Qdrant (no model, no worker).

    .venv-nlp/bin/python tools/nlp/similar.py build  [--k 5] [--min-score 0.86]
    .venv-nlp/bin/python tools/nlp/similar.py review [--per-band 12]
    .venv-nlp/bin/python tools/nlp/similar.py score

A document is the normalised mean of its chunk vectors. For each document the k nearest other
documents are kept; a pair is stored once (undirected), never with itself, never below
`--min-score`. Each pair carries the best matching chunk of each side as evidence. The cosine
is a ranking signal, not a probability: it is never shown as a percentage.

`build` writes data/similar.jsonl (not in git). `review` draws a sample stratified by score
band into eval/similar/review.tsv for a person to mark `1` (really about the same thing) or
`0`; `score` turns the marked file into precision per band, which is what the threshold must
be chosen from. Nothing here enters the graph before that has been done.
"""
import argparse
import csv
import json
import os
import random
import re
import subprocess
import urllib.request
from collections import defaultdict
from pathlib import Path

import numpy as np

from common import COLLECTION as DEFAULT_COLLECTION

COLLECTION = os.environ.get("QDRANT_COLLECTION", DEFAULT_COLLECTION)

QDRANT = os.environ.get("QDRANT_URL", "http://127.0.0.1:6333")


def qdrant(method, path, payload=None):
    request = urllib.request.Request(
        QDRANT + path, method=method, data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)

DATA = Path("data/similar.jsonl")
REVIEW = Path("eval/similar/review.tsv")
BANDS = [(0.86, 0.93), (0.93, 0.95), (0.95, 0.97), (0.97, 1.01)]  # e5 cosines sit close to 1


def load():
    """doc_id -> list of (ordinal, vector), plus the db ids."""
    docs, db_ids, offset = defaultdict(list), {}, None
    while True:
        body = {"limit": 500, "with_payload": ["doc_id", "db_id", "ordinal"], "with_vector": True}
        if offset is not None:
            body["offset"] = offset
        result = qdrant("POST", f"/collections/{COLLECTION}/points/scroll", body)["result"]
        for point in result["points"]:
            payload = point["payload"]
            docs[payload["doc_id"]].append((payload["ordinal"], np.asarray(point["vector"], dtype=np.float32)))
            db_ids[payload["doc_id"]] = payload["db_id"]
        offset = result.get("next_page_offset")
        if offset is None:
            return docs, db_ids


def titles():
    sql = "COPY (SELECT source||':'||native_id, title, canonical_url FROM documents) TO STDOUT"
    out = subprocess.run(
        ["docker-compose", "exec", "-T", "postgres", "psql", "-U", "rks", "-d", "rks", "-c", sql],
        capture_output=True, text=True, check=True).stdout
    return {p[0]: (p[1], p[2]) for p in (l.split("\t") for l in out.splitlines()) if len(p) == 3}


def template(title):
    """The title with numbers removed: recurring series ("Rust 1.42.0 pre-release testing",
    "Triage digest: Mon Feb 01 2016") collapse to one string and are not "similar" topics."""
    words = re.sub(r"[0-9]+", " ", title.lower()).split()
    return re.sub(r"[^a-z]+", "", "".join(w for w in words if w not in MONTHS and w not in DAYS))


MONTHS = {"jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"}
DAYS = {"mon", "tue", "wed", "thu", "fri", "sat", "sun"}


def build(args):
    docs, db_ids = load()
    names = titles()
    ids = sorted(docs)
    matrix = np.stack([np.mean([v for _, v in docs[i]], axis=0) for i in ids])
    matrix /= np.linalg.norm(matrix, axis=1, keepdims=True)
    pairs, dropped = {}, [0]
    for start in range(0, len(ids), 500):
        sims = matrix[start:start + 500] @ matrix.T
        for row, i in enumerate(range(start, min(start + 500, len(ids)))):
            sims[row, i] = -1.0  # no self edge
            for j in np.argsort(-sims[row])[: args.k]:
                score = float(sims[row, j])
                if score >= args.min_score:
                    a, b = sorted((ids[i], ids[int(j)]))
                    if template(names.get(a, ("",))[0]) == template(names.get(b, ("",))[0]):
                        dropped[0] += 1
                        continue
                    pairs[(a, b)] = score
    DATA.parent.mkdir(exist_ok=True)
    with DATA.open("w") as out:
        for (a, b), score in sorted(pairs.items()):
            best = max(((x @ y, ox, oy) for ox, x in docs[a] for oy, y in docs[b]), key=lambda t: t[0])
            out.write(json.dumps({
                "a": a, "b": b, "score": round(score, 4), "chunk_a": best[1], "chunk_b": best[2],
                "chunk_score": round(float(best[0]), 4),
                "model": "intfloat/multilingual-e5-small", "collection": COLLECTION,
                "method": f"mean-chunk-cosine,k={args.k},min={args.min_score}"}) + "\n")
    print(f"{len(ids)} documents, {len(pairs)} pairs -> {DATA} ({dropped[0]} template-series pairs dropped)")
    for low, high in BANDS:
        print(f"  [{low:.2f}, {min(high, 1.0):.2f}): {sum(low <= s < high for s in pairs.values())}")


def read_pairs():
    return [json.loads(line) for line in DATA.open()]


def review(args):
    rng = random.Random(7)
    pairs, names, rows = read_pairs(), titles(), []
    for low, high in BANDS:
        band = [p for p in pairs if low <= p["score"] < high]
        for p in rng.sample(band, min(args.per_band, len(band))):
            rows.append((p, f"{low:.2f}"))
    rng.shuffle(rows)  # the reviewer sees no score order
    REVIEW.parent.mkdir(parents=True, exist_ok=True)
    with REVIEW.open("w", newline="") as out:
        writer = csv.writer(out, delimiter="\t")
        writer.writerow(["pair", "a", "title_a", "b", "title_b", "band", "same_topic", "silver_same_topic"])
        for n, (p, band) in enumerate(rows):
            writer.writerow([n, p["a"], names.get(p["a"], ("?",))[0], p["b"], names.get(p["b"], ("?",))[0], band, "", ""])
    print(f"{len(rows)} pairs -> {REVIEW}; fill `same_topic` with 1 or 0")


def score(_):
    stats = defaultdict(lambda: [0, 0])
    for row in csv.DictReader(REVIEW.open(), delimiter="\t"):
        for column in ("same_topic", "silver_same_topic"):
            if row.get(column) in ("0", "1"):
                stats[(row["band"], column)][0] += int(row[column])
                stats[(row["band"], column)][1] += 1
    for (band, column), (good, total) in sorted(stats.items()):
        print(f"band >= {band} [{column}]: precision {good}/{total} = {good / total:.2f}")
    if not stats:
        print("no marked pairs yet")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build")
    b.add_argument("--k", type=int, default=5)
    b.add_argument("--min-score", type=float, default=0.86)
    r = sub.add_parser("review")
    r.add_argument("--per-band", type=int, default=12)
    sub.add_parser("score")
    args = parser.parse_args()
    {"build": build, "review": review, "score": score}[args.command](args)
