#!/usr/bin/env python3
"""Chunks the corpus, embeds the chunks and loads them into Qdrant.

    .venv-nlp/bin/python tools/nlp/embed_corpus.py --collection NEW_COLLECTION [--limit N]

Chunks follow the posts: consecutive posts are packed up to CHUNK_CHARS, a longer post is cut
at paragraph or sentence ends. Every chunk carries the title (the topic is part of its
meaning). All chunks are embedded. The model's token limit may still truncate individual inputs.
Stable UUID point IDs support any number of chunks. A new collection name is required and an
existing collection is never overwritten. The database stays the source of truth.
Run tools/nlp/set_year.py afterwards: the period filter needs the `year` payload.
"""
import argparse
import os
import hashlib
import json
import re
import subprocess
import sys
import time
import urllib.request
import urllib.error
import uuid
from pathlib import Path

from common import CHUNKER_VERSION, DIM, MODEL, embed, load_model

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
    if proc.wait() != 0:
        raise RuntimeError("failed to read corpus from PostgreSQL")


def qdrant(method, path, payload=None):
    request = urllib.request.Request(
        QDRANT + path, method=method, data=json.dumps(payload).encode() if payload is not None else None,
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def point_id(doc_db_id, ordinal):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"rust-knowledge:{doc_db_id}:{ordinal}"))


def corpus_hash(limit):
    digest = hashlib.sha256()
    for meta, posts in documents(limit):
        digest.update(json.dumps([meta, posts], ensure_ascii=False, sort_keys=True).encode())
        digest.update(b"\n")
    return digest.hexdigest()


def existing_points(collection):
    points = {}
    offset = None
    while True:
        body = {"limit": 500, "with_payload": True, "with_vector": False}
        if offset is not None:
            body["offset"] = offset
        page = qdrant("POST", f"/collections/{collection}/points/scroll", body)["result"]
        for point in page["points"]:
            points[str(point["id"])] = point["payload"]
        offset = page.get("next_page_offset")
        if offset is None:
            return points


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--collection", required=True, help="new Qdrant collection name (must not exist)")
    parser.add_argument("--limit", type=int, default=0, help="documents (0 = all)")
    parser.add_argument("--resume", action="store_true", help="continue an interrupted collection")
    parser.add_argument("--adopt-existing", action="store_true", help="adopt a legacy interrupted collection without a checkpoint; model provenance cannot be verified")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_-]+", args.collection):
        parser.error("collection name must contain only letters, digits, _ and -")
    collection = args.collection
    if args.adopt_existing and not args.resume:
        parser.error("--adopt-existing requires --resume")
    checkpoint = Path(__file__).resolve().parents[2] / "data" / f"vectors-{collection}.checkpoint.json"
    final = checkpoint.with_name(f"vectors-{collection}.json")
    if final.exists():
        parser.error("collection already has a completed manifest")
    try:
        info = qdrant("GET", f"/collections/{collection}")["result"]
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        info = None
    if args.resume and info is None:
        parser.error("collection does not exist; cannot resume")
    if not args.resume and info is not None:
        parser.error(f"collection {collection} already exists; choose a new name")
    if args.resume and not checkpoint.exists() and not args.adopt_existing:
        parser.error("checkpoint missing; legacy collections require --adopt-existing")

    model = load_model()
    revision = getattr(model[0].auto_model.config, "_commit_hash", None)
    config = {"collection": collection, "model": MODEL, "model_revision": revision,
              "dim": DIM, "chunker": CHUNKER_VERSION, "chunk_chars": CHUNK_CHARS,
              "max_seq_length": model.max_seq_length, "limit": args.limit, "corpus_sha256": corpus_hash(args.limit)}
    if args.resume:
        if info["config"]["params"]["vectors"]["size"] != DIM or info["config"]["params"]["vectors"]["distance"] != "Cosine":
            parser.error("collection vector configuration does not match")
        if checkpoint.exists():
            saved = json.loads(checkpoint.read_text())
            if saved["config"] != config:
                parser.error("checkpoint mismatch: corpus, model or settings changed")
        else:
            print("WARNING: legacy collection has no checkpoint; original model and full corpus cannot be verified", file=sys.stderr)
        existing = existing_points(collection)
        unseen = set(existing)
        for meta, posts in documents(args.limit):
            for ordinal, text in enumerate(chunk(posts)):
                key = point_id(meta["db_id"], ordinal)
                if key in existing:
                    expected = {k: meta[k] for k in ("db_id", "doc_id", "source", "completeness")}
                    expected.update({"ordinal": ordinal, "text": text})
                    if any(existing[key].get(k) != v for k, v in expected.items()):
                        raise RuntimeError(f"existing chunk differs from current corpus: {meta['doc_id']} / {ordinal}")
                    unseen.remove(key)
        if unseen:
            raise RuntimeError(f"collection contains {len(unseen)} points absent from current corpus")
        if not checkpoint.exists():
            checkpoint.write_text(json.dumps({"config": config, "legacy_provenance_unverified": True}, indent=2))
    else:
        checkpoint.write_text(json.dumps({"config": config, "legacy_provenance_unverified": False}, indent=2), encoding="utf-8")
        qdrant("PUT", f"/collections/{collection}", {"vectors": {"size": DIM, "distance": "Cosine"}})
        for field in ("source", "completeness", "doc_id"):
            qdrant("PUT", f"/collections/{collection}/index", {"field_name": field, "field_schema": "keyword"})
        existing = {}

    started, docs, chunks_total = time.time(), 0, 0
    batch = []

    def flush():
        nonlocal batch
        if not batch:
            return
        vectors = embed(model, [b["passage"] for b in batch], "passage")
        points = [{"id": point_id(b["db_id"], b["ordinal"]), "vector": v,
                   "payload": {k: b[k] for k in ("db_id", "doc_id", "source", "completeness", "ordinal", "text")}}
                  for b, v in zip(batch, vectors)]
        qdrant("PUT", f"/collections/{collection}/points?wait=true", {"points": points})
        batch = []

    for meta, posts in documents(args.limit):
        pieces = chunk(posts)
        for ordinal, text in enumerate(pieces):
            chunks_total += 1
            if point_id(meta["db_id"], ordinal) in existing:
                continue
            batch.append({**meta, "ordinal": ordinal, "text": text,
                          "passage": f"{meta['title']}. {text}"})
            if len(batch) >= 32:
                flush()
        docs += 1
        if docs % 100 == 0:
            print(f"{docs} documents, {chunks_total} chunks, {time.time() - started:.0f}s", file=sys.stderr, flush=True)
    flush()
    if corpus_hash(args.limit) != config["corpus_sha256"]:
        raise RuntimeError("corpus changed during embedding; collection not marked complete")
    manifest = {"collection": collection, "model": MODEL, "model_revision": revision, "dim": DIM,
                "chunker": CHUNKER_VERSION, "chunk_chars": CHUNK_CHARS,
                "max_chunks_per_doc": None, "documents": docs,
                "chunks": chunks_total, "documents_truncated": 0,
                "corpus_sha256": config["corpus_sha256"],
                "legacy_provenance_unverified": json.loads(checkpoint.read_text())["legacy_provenance_unverified"],
                "built_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "seconds": round(time.time() - started)}
    manifest["sha256"] = hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest()
    with final.open("x") as f:
        json.dump(manifest, f, indent=2)
    checkpoint.unlink()
    print(json.dumps(manifest, indent=2), file=sys.stderr)


if __name__ == "__main__":
    main()
