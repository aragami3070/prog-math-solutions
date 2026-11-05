#!/usr/bin/env python3
"""Chunks the corpus, embeds the chunks and loads them into Qdrant.

    .venv-nlp/bin/python tools/nlp/embed_corpus.py [--max-chunks-per-doc 6] [--limit N]

Chunks follow the posts: consecutive posts are packed up to CHUNK_CHARS, a longer post is cut
at paragraph or sentence ends. Every chunk carries the title (the topic is part of its
meaning). Only the first MAX_CHUNKS chunks of a document are embedded: this bounds cost on a
small machine, and it is a stated limit, not a silent one (see data/vectors.json).
Qdrant ids are stable (document id, ordinal), so a rerun overwrites instead of duplicating.
The collection is rebuilt from scratch each run; the database stays the source of truth.
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
import time
import urllib.request

from common import CHUNKER_VERSION, COLLECTION, DIM, MODEL, embed, load_model

CHUNK_CHARS = 1100
QDRANT = "http://127.0.0.1:6333"
SQL = r"""COPY (
  SELECT d.id, d.source || ':' || d.native_id, d.source, d.completeness, d.title,
         regexp_replace(p.text, E'[\\r\\n\\t\\\\]+', ' ', 'g')
  FROM documents d JOIN posts p ON p.document_id = d.id
  ORDER BY d.id, p.post_number) TO STDOUT"""


def split_long(text):
    pieces, current = [], ""
    for sentence in re.split(r"(?<=[.!?])\s+", text):
        if len(current) + len(sentence) + 1 > CHUNK_CHARS and current:
            pieces.append(current)
            current = ""
        while len(sentence) > CHUNK_CHARS:  # code without sentence ends
            pieces.append(sentence[:CHUNK_CHARS])
            sentence = sentence[CHUNK_CHARS:]
        current = (current + " " + sentence).strip()
    if current:
        pieces.append(current)
    return pieces


def chunk(posts):
    chunks, current = [], ""
    for text in posts:
        for piece in split_long(text) if len(text) > CHUNK_CHARS else [text]:
            if current and len(current) + len(piece) + 1 > CHUNK_CHARS:
                chunks.append(current)
                current = ""
            current = (current + " " + piece).strip()
    if current:
        chunks.append(current)
    return chunks


def documents(limit):
    proc = subprocess.Popen(
        ["docker-compose", "exec", "-T", "postgres", "psql", "-U", "rks", "-d", "rks", "-c", SQL],
        stdout=subprocess.PIPE, text=True)
    current, posts, count = None, [], 0
    for line in proc.stdout:
        parts = line.rstrip("\n").split("\t")
        if len(parts) != 6:
            continue
        db_id, doc_id, source, completeness, title, text = parts
        if current and current["db_id"] != int(db_id):
            yield current, posts
            count += 1
            if limit and count >= limit:
                proc.kill()
                return
            posts = []
        current = {"db_id": int(db_id), "doc_id": doc_id, "source": source,
                   "completeness": completeness, "title": title}
        posts.append(text)
    if current:
        yield current, posts
    proc.wait()


def qdrant(method, path, payload=None):
    request = urllib.request.Request(
        QDRANT + path, method=method, data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def point_id(doc_db_id, ordinal):
    return doc_db_id * 100 + ordinal  # ordinal < 100


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--max-chunks-per-doc", type=int, default=6)
    parser.add_argument("--limit", type=int, default=0, help="documents (0 = all)")
    args = parser.parse_args()
    assert args.max_chunks_per_doc < 100

    model = load_model()
    revision = getattr(model[0].auto_model.config, "_commit_hash", None)
    try:
        qdrant("DELETE", f"/collections/{COLLECTION}")
    except Exception:
        pass
    qdrant("PUT", f"/collections/{COLLECTION}", {"vectors": {"size": DIM, "distance": "Cosine"}})
    for field in ("source", "completeness", "doc_id"):
        qdrant("PUT", f"/collections/{COLLECTION}/index", {"field_name": field, "field_schema": "keyword"})

    started, docs, chunks_total, truncated = time.time(), 0, 0, 0
    batch = []

    def flush():
        nonlocal batch
        if not batch:
            return
        vectors = embed(model, [b["passage"] for b in batch], "passage")
        points = [{"id": point_id(b["db_id"], b["ordinal"]), "vector": v,
                   "payload": {k: b[k] for k in ("db_id", "doc_id", "source", "completeness", "ordinal", "text")}}
                  for b, v in zip(batch, vectors)]
        qdrant("PUT", f"/collections/{COLLECTION}/points?wait=true", {"points": points})
        batch = []

    for meta, posts in documents(args.limit):
        pieces = chunk(posts)
        truncated += len(pieces) > args.max_chunks_per_doc
        for ordinal, text in enumerate(pieces[: args.max_chunks_per_doc]):
            batch.append({**meta, "ordinal": ordinal, "text": text[:600],
                          "passage": f"{meta['title']}. {text}"})
            chunks_total += 1
            if len(batch) >= 32:
                flush()
        docs += 1
        if docs % 100 == 0:
            print(f"{docs} documents, {chunks_total} chunks, {time.time() - started:.0f}s", file=sys.stderr, flush=True)
    flush()
    manifest = {"collection": COLLECTION, "model": MODEL, "model_revision": revision, "dim": DIM,
                "chunker": CHUNKER_VERSION, "chunk_chars": CHUNK_CHARS,
                "max_chunks_per_doc": args.max_chunks_per_doc, "documents": docs,
                "chunks": chunks_total, "documents_truncated": truncated,
                "built_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "seconds": round(time.time() - started)}
    manifest["sha256"] = hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()
    with open("data/vectors.json", "w") as f:
        json.dump(manifest, f, indent=2)
    print(json.dumps(manifest, indent=2), file=sys.stderr)


if __name__ == "__main__":
    main()
