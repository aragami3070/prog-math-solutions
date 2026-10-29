-- A page the source says does not exist (404/410) is a dead link, not a failure to retry:
-- it is settled as 'missing' and counted apart from errors.
ALTER TABLE crawl_queue DROP CONSTRAINT crawl_queue_status_check;
ALTER TABLE crawl_queue
    ADD CONSTRAINT crawl_queue_status_check CHECK (status IN ('pending', 'done', 'failed', 'missing'));
ALTER TABLE crawl_runs ADD COLUMN missing INTEGER NOT NULL DEFAULT 0 CHECK (missing >= 0);

UPDATE crawl_queue SET status = 'missing'
WHERE status = 'failed' AND error IN ('document returned HTTP 404', 'document returned HTTP 410');

UPDATE crawl_runs AS r
SET missing = m.pages, failed = GREATEST(r.failed - m.pages, 0)
FROM (
    SELECT run_id, COUNT(*)::INTEGER AS pages FROM crawl_queue WHERE status = 'missing' GROUP BY run_id
) AS m
WHERE m.run_id = r.id;

-- A run that only failed on dead links has nothing left to retry.
UPDATE crawl_runs AS r SET status = 'completed', error = NULL
WHERE r.status = 'failed' AND r.failed = 0 AND r.missing > 0
  AND NOT EXISTS (SELECT 1 FROM crawl_queue AS q WHERE q.run_id = r.id AND q.status IN ('pending', 'failed'));
