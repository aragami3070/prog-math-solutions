-- Machine outputs and human decisions are separate: reruns never update reviews.
CREATE TABLE classification_runs (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL CHECK (content_hash <> ''),
    taxonomy_version TEXT NOT NULL CHECK (taxonomy_version <> ''),
    pipeline_version TEXT NOT NULL CHECK (pipeline_version <> ''),
    status TEXT NOT NULL CHECK (status IN ('proposed', 'unclassified', 'failed')),
    provenance JSONB NOT NULL CHECK (jsonb_typeof(provenance) = 'object'),
    coverage DOUBLE PRECISION CHECK (coverage >= 0 AND coverage <= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (document_id, content_hash, taxonomy_version, pipeline_version)
);
CREATE INDEX classification_runs_document_idx ON classification_runs (document_id, created_at DESC, id DESC);

CREATE TABLE classification_topics (
    run_id BIGINT NOT NULL REFERENCES classification_runs(id) ON DELETE CASCADE,
    topic_id TEXT NOT NULL CHECK (topic_id <> ''),
    evidence JSONB NOT NULL CHECK (jsonb_typeof(evidence) = 'array'),
    PRIMARY KEY (run_id, topic_id)
);

-- Append new decisions; the latest event for a document/taxonomy/topic wins.
-- A manual addition may refer to no machine run, but always records the hash reviewed.
CREATE TABLE classification_review_events (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    taxonomy_version TEXT NOT NULL CHECK (taxonomy_version <> ''),
    content_hash TEXT NOT NULL CHECK (content_hash <> ''),
    topic_id TEXT NOT NULL CHECK (topic_id <> ''),
    decision TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected', 'added')),
    reviewer TEXT NOT NULL CHECK (reviewer <> ''),
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(evidence) = 'array'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX classification_review_events_lookup_idx
    ON classification_review_events (document_id, taxonomy_version, topic_id, id DESC);
