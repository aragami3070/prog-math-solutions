-- Outgoing links of a post, with attribution and anchor text.
CREATE TABLE links (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    target_url TEXT NOT NULL,
    anchor_text TEXT,
    UNIQUE (post_id, target_url)
);
CREATE INDEX links_document_idx ON links(document_id);
CREATE INDEX links_target_idx ON links(target_url);
