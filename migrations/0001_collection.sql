CREATE TABLE crawl_sources (
    source TEXT PRIMARY KEY CHECK (source IN ('internals', 'rfc')),
    paused BOOLEAN NOT NULL DEFAULT FALSE,
    pause_reason TEXT
);
INSERT INTO crawl_sources (source) VALUES ('internals'), ('rfc');

CREATE TABLE crawl_runs (
    id BIGSERIAL PRIMARY KEY,
    source TEXT NOT NULL REFERENCES crawl_sources(source),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'blocked', 'interrupted')),
    requested INTEGER NOT NULL CHECK (requested BETWEEN 1 AND 20),
    succeeded INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    error TEXT
);

-- HTML and policy responses are preserved for offline re-parsing. PostgreSQL
-- TOAST compresses large text; external object storage is not needed yet.
CREATE TABLE fetches (
    id BIGSERIAL PRIMARY KEY,
    run_id BIGINT NOT NULL REFERENCES crawl_runs(id),
    url TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('robots', 'document')),
    status INTEGER,
    content_type TEXT,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    body TEXT,
    error TEXT
);
CREATE INDEX fetches_run_idx ON fetches(run_id);

CREATE TABLE documents (
    id BIGSERIAL PRIMARY KEY,
    source TEXT NOT NULL REFERENCES crawl_sources(source),
    native_id TEXT NOT NULL,
    canonical_url TEXT NOT NULL,
    title TEXT NOT NULL,
    body_text TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    completeness TEXT NOT NULL CHECK (completeness IN ('complete', 'partial', 'unknown')),
    completeness_reason TEXT NOT NULL,
    metadata JSONB NOT NULL,
    parser_version TEXT NOT NULL,
    fetch_id BIGINT NOT NULL REFERENCES fetches(id),
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source, native_id)
);
CREATE INDEX documents_order_idx ON documents(first_seen_at DESC, id DESC);
