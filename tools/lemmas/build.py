#!/usr/bin/env python3
"""Builds the lemma table the search uses for word-form matching.

spaCy tags every word of the corpus in context (so `objects` as a noun and `objects` as a
verb are told apart by their sentence); for each surface form the table keeps the lemma it
most often got. The table is derived data: regenerate it with this script, it is not in git.
Search reads it at start-up; without it the lemma mode reports itself unavailable.

    python3 -m venv --without-pip .venv-nlp && ... (see tools/lemmas/README.md)
    .venv-nlp/bin/python tools/lemmas/build.py data/lemmas.json
"""
import collections
import hashlib
import json
import re
import subprocess
import sys

import spacy

VERSION = "lemmas-v1"  # v1: a sample of 800 documents
MODEL = "en_core_web_sm"
import os
SAMPLE = int(os.environ.get("LEMMA_SAMPLE", "800"))  # documents; a fixed pseudo-random sample, the vocabulary is covered long before the end
SQL = r"""COPY (SELECT regexp_replace(body_text, E'[\\r\\n\\t\\\\]+', ' ', 'g') FROM documents ORDER BY md5(id::text) LIMIT {limit}) TO STDOUT"""
WORD = re.compile(r"^[a-z]{3,}$")


def texts():
    proc = subprocess.Popen(
        ["docker-compose", "exec", "-T", "postgres", "psql", "-U", "rks", "-d", "rks", "-c", SQL.format(limit=SAMPLE)],
        stdout=subprocess.PIPE, text=True)
    for line in proc.stdout:
        yield line.rstrip("\n")[:200_000]
    proc.wait()


def main(out_path):
    nlp = spacy.load(MODEL, disable=["parser", "ner"])
    counts = collections.defaultdict(collections.Counter)  # surface -> Counter[(lemma, pos)]
    docs = 0
    for doc in nlp.pipe(texts(), batch_size=8):
        docs += 1
        for token in doc:
            surface = token.text.lower()
            if WORD.match(surface) and token.pos_ in {"NOUN", "VERB", "ADJ", "ADV", "AUX"}:
                counts[surface][(token.lemma_.lower(), token.pos_)] += 1
        if docs % 200 == 0:
            print(f"{docs} documents", file=sys.stderr, flush=True)
    table = {}
    for surface, counter in counts.items():
        (lemma, pos), _ = counter.most_common(1)[0]
        if lemma != surface and WORD.match(lemma):
            table[surface] = {"lemma": lemma, "pos": pos}
    payload = {"version": VERSION, "model": MODEL, "spacy": spacy.__version__,
               "documents": docs, "forms": table}
    data = json.dumps(payload, sort_keys=True, ensure_ascii=False)
    payload["sha256"] = hashlib.sha256(data.encode()).hexdigest()
    with open(out_path, "w") as f:
        json.dump(payload, f, sort_keys=True, ensure_ascii=False)
    print(f"{len(table)} forms with a different lemma, sha256 {payload['sha256']}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
