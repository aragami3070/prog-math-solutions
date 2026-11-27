-- Durable selection and call ledger for resumable classification batches.
CREATE TABLE classification_batches (
    id TEXT PRIMARY KEY,
    pipeline_version TEXT NOT NULL,
    max_calls INTEGER NOT NULL CHECK (max_calls > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE classification_batch_documents (
    batch_id TEXT NOT NULL REFERENCES classification_batches(id),
    document_id BIGINT NOT NULL REFERENCES documents(id),
    PRIMARY KEY (batch_id, document_id)
);
CREATE TABLE classification_call_attempts (
    id BIGSERIAL PRIMARY KEY,
    batch_id TEXT NOT NULL REFERENCES classification_batches(id),
    document_id BIGINT NOT NULL REFERENCES documents(id),
    content_hash TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    outcome TEXT NOT NULL DEFAULT 'interrupted',
    usage JSONB,
    result JSONB,
    detail TEXT
);
CREATE INDEX classification_call_attempts_batch_idx ON classification_call_attempts(batch_id);
