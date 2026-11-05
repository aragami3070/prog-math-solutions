#!/usr/bin/env python3
"""Query-embedding service: a small local HTTP server around the embedding model.

    POST /embed   {"texts": ["..."], "kind": "query"}  ->  {"model": ..., "vectors": [[...]]}
    GET  /health  ->  {"model": ..., "dim": ...}

Listens on loopback only. The Rust API calls it for every vector or hybrid query; when it is
down the API answers lexically and says so.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from common import DIM, MODEL, embed, load_model

MAX_TEXTS = 64
MAX_CHARS = 2000


class Handler(BaseHTTPRequestHandler):
    model = None

    def _send(self, status, payload):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            self._send(200, {"model": MODEL, "dim": DIM})
        else:
            self._send(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/embed":
            return self._send(404, {"error": "not found"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
            request = json.loads(self.rfile.read(min(length, 1_000_000)))
            texts, kind = request["texts"], request.get("kind", "query")
            if kind not in ("query", "passage") or not isinstance(texts, list) or not 0 < len(texts) <= MAX_TEXTS:
                raise ValueError("bad request")
            texts = [str(t)[:MAX_CHARS] for t in texts]
        except Exception:
            return self._send(400, {"error": "bad request"})
        self._send(200, {"model": MODEL, "vectors": embed(self.model, texts, kind)})

    def log_message(self, *args):
        pass


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8090
    Handler.model = load_model()
    print(f"embedding worker on 127.0.0.1:{port}", file=sys.stderr, flush=True)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main()
