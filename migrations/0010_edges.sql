-- Document graph. Only `references` is built here: a link in one document's posts that points
-- at another document of the corpus. Rebuilt whole from `links`, so it never goes stale; the
-- evidence (a few of the linking posts) stays with the edge. Fragments (#post, #footnote) are
-- evidence, not identity: two links to different anchors of one document are one edge.
CREATE TABLE edges (
    id          BIGSERIAL PRIMARY KEY,
    source_id   BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    target_id   BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('references')),
    occurrences INTEGER NOT NULL CHECK (occurrences >= 1),
    evidence    JSONB NOT NULL DEFAULT '[]',
    built_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (source_id <> target_id),
    UNIQUE (source_id, target_id, kind)
);
CREATE INDEX edges_target_idx ON edges (target_id);
