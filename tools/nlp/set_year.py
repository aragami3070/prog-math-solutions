#!/usr/bin/env python3
"""Adds the `year` payload (year of a document's first post, UTC) to the chunks in Qdrant.

    .venv-nlp/bin/python tools/nlp/set_year.py

The period filter of the search reads it. No embeddings are recomputed. Documents without a
date (all RFCs) get no `year`, so a year bound never matches them: the same rule as in the
Tantivy index. Run it after every embed_corpus.py run (that script rebuilds the collection).
"""
import json
import os
import subprocess
import urllib.request
from collections import defaultdict

QDRANT = os.environ.get("QDRANT_URL", "http://127.0.0.1:6333")
COLLECTION = os.environ.get("QDRANT_COLLECTION", "rks_chunks_v1")
SQL = """COPY (
  SELECT d.source || ':' || d.native_id,
         EXTRACT(YEAR FROM p.published_at AT TIME ZONE 'UTC')::INT
  FROM documents d JOIN posts p ON p.document_id = d.id AND p.post_number = 1
  WHERE p.published_at IS NOT NULL) TO STDOUT"""


def qdrant(method, path, payload):
    request = urllib.request.Request(
        QDRANT + path, method=method, data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=300) as response:
        return json.load(response)


def main():
    out = subprocess.run(
        ["docker-compose", "exec", "-T", "postgres", "psql", "-U", "rks", "-d", "rks", "-c", SQL],
        capture_output=True, text=True, check=True).stdout
    by_year = defaultdict(list)
    for line in out.splitlines():
        doc_id, year = line.split("\t")
        by_year[int(year)].append(doc_id)
    qdrant("PUT", f"/collections/{COLLECTION}/index?wait=true", {"field_name": "year", "field_schema": "integer"})
    for year, docs in sorted(by_year.items()):
        qdrant("POST", f"/collections/{COLLECTION}/points/payload?wait=true", {
            "payload": {"year": year},
            "filter": {"must": [{"key": "doc_id", "match": {"any": docs}}]}})
        print(year, len(docs), "documents")


if __name__ == "__main__":
    main()
