-- Online evaluation events (P10). No IP, no User-Agent: a session is a random string the
-- browser makes for itself. A click copies session, experiment and arm from its search row,
-- so a client cannot attach a click to an arm it was not assigned.
CREATE TABLE search_events (
    id             BIGSERIAL PRIMARY KEY,
    at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind           TEXT NOT NULL CHECK (kind IN ('search', 'click')),
    session_id     TEXT NOT NULL CHECK (char_length(session_id) BETWEEN 8 AND 64),
    experiment     TEXT NOT NULL,
    variant        TEXT NOT NULL,
    query_id       UUID NOT NULL DEFAULT gen_random_uuid(),
    -- search
    query          TEXT,
    mode           TEXT,
    requested_mode TEXT,
    source         TEXT,
    complete_only  BOOLEAN,
    results        INTEGER,
    latency_ms     INTEGER,
    degraded       BOOLEAN,
    generation     TEXT,
    -- click
    document_id    TEXT,
    rank           INTEGER CHECK (rank IS NULL OR rank >= 1),
    CHECK (kind <> 'search' OR (query IS NOT NULL AND results IS NOT NULL)),
    CHECK (kind <> 'click' OR (document_id IS NOT NULL AND rank IS NOT NULL))
);
CREATE UNIQUE INDEX search_events_query_idx ON search_events (query_id) WHERE kind = 'search';
CREATE INDEX search_events_experiment_idx ON search_events (experiment, at);
CREATE INDEX search_events_session_idx ON search_events (session_id, at);
