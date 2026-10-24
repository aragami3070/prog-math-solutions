-- Looked up by page URL to decide whether a stored response is still fresh.
CREATE INDEX document_pages_url_idx ON document_pages(url);
