ALTER TABLE crawl_runs DROP CONSTRAINT crawl_runs_requested_check;
ALTER TABLE crawl_runs ADD CHECK (requested >= 0);

CREATE TABLE crawl_queue (
    id BIGSERIAL PRIMARY KEY,
    run_id BIGINT NOT NULL REFERENCES crawl_runs(id),
    url TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'failed')),
    error TEXT,
    UNIQUE (run_id, url)
);
CREATE INDEX crawl_queue_pending ON crawl_queue (run_id, id) WHERE status = 'pending';
