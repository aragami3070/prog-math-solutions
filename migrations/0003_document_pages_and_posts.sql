-- One row per fetched HTTP page of a document (Internals topic page, or the single RFC page).
-- Pages accumulate across runs: re-fetching replaces only that page.
CREATE TABLE document_pages (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_no INTEGER NOT NULL CHECK (page_no >= 1),
    url TEXT NOT NULL,
    fetch_id BIGINT NOT NULL REFERENCES fetches(id),
    has_next BOOLEAN NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (document_id, page_no)
);

-- One row per post. The text lives here once; documents.body_text is derived from it.
CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_id BIGINT NOT NULL REFERENCES document_pages(id) ON DELETE CASCADE,
    post_number INTEGER NOT NULL CHECK (post_number >= 1),
    author TEXT,
    published_at TIMESTAMPTZ,
    text TEXT NOT NULL,
    code JSONB NOT NULL DEFAULT '[]',
    UNIQUE (document_id, post_number)
);
CREATE INDEX posts_page_idx ON posts(page_id);

-- Superseded by document_pages.fetch_id; dropped together with the legacy JSON later.
ALTER TABLE documents ALTER COLUMN fetch_id DROP NOT NULL;
