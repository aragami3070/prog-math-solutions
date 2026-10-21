-- Posts and pages now live in their own tables. Keep only the license pointer.
UPDATE documents
SET metadata = jsonb_build_object('license_source', metadata->'license_source');

-- Superseded by document_pages.fetch_id.
ALTER TABLE documents DROP COLUMN fetch_id;
