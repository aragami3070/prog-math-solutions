-- `url` stays the canonical identity of a page (no topic slug) and keeps deduplicating.
-- `fetch_url` is what is actually requested: Discourse redirects /t/ID to /t/slug/ID,
-- which doubled the requests (and the polite delay) for every topic page.
ALTER TABLE crawl_queue ADD COLUMN fetch_url TEXT;
UPDATE crawl_queue SET fetch_url = url;

-- Pages queued before this migration: reuse a slug seen in stored responses. The URL of a
-- fetched page is authoritative (rank 0); links inside any stored page may be stale (rank 1).
UPDATE crawl_queue AS q
SET fetch_url = 'https://internals.rust-lang.org/t/' || s.slug || '/' || s.topic_id
                || COALESCE(substring(q.url FROM '\?.*$'), '')
FROM (
    SELECT DISTINCT ON (topic_id) topic_id, slug
    FROM (
        SELECT
            (regexp_match(url, '^https://internals\.rust-lang\.org/t/([^/?]+)/([0-9]+)(?:\?.*)?$'))[1] AS slug,
            (regexp_match(url, '^https://internals\.rust-lang\.org/t/([^/?]+)/([0-9]+)(?:\?.*)?$'))[2] AS topic_id,
            0 AS rank
        FROM fetches
        WHERE kind = 'document' AND status = 200
        UNION ALL
        SELECT m[1], m[2], 1
        FROM fetches AS f,
             LATERAL regexp_matches(
                 f.body,
                 'href=[''"]https://internals\.rust-lang\.org/t/([^/''"?#]+)/([0-9]+)',
                 'g'
             ) AS m
        WHERE f.kind = 'document' AND f.status = 200
    ) AS found
    WHERE slug IS NOT NULL AND slug !~ '^[0-9]+$'
    ORDER BY topic_id, rank, slug
) AS s
WHERE q.status = 'pending'
  AND (regexp_match(q.url, '^https://internals\.rust-lang\.org/t/([0-9]+)(?:\?.*)?$'))[1] = s.topic_id;

ALTER TABLE crawl_queue ALTER COLUMN fetch_url SET NOT NULL;
