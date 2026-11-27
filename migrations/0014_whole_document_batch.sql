-- Immutable configuration for whole-document batches; existing chunk batches remain untouched.
CREATE TABLE classification_whole_batch_config (
    batch_id TEXT PRIMARY KEY REFERENCES classification_batches(id),
    selection_batch_id TEXT NOT NULL REFERENCES classification_batches(id),
    taxonomy_hash TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    max_chars INTEGER NOT NULL CHECK (max_chars > 0),
    model TEXT NOT NULL
);
