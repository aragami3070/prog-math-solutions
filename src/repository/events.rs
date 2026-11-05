use crate::models::events::{EventRow, NewSearch};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};

/// Records a search and returns its `query_id`.
pub async fn insert_search(
    tx: &mut Transaction<'_, Postgres>,
    experiment: &str,
    variant: &str,
    search: &NewSearch<'_>,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO search_events (
            kind, session_id, experiment, variant, query, mode, requested_mode, source,
            complete_only, results, latency_ms, degraded, generation
        )
        VALUES ('search', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING query_id::text AS "query_id!"
        "#,
        search.session_id,
        experiment,
        variant,
        search.query,
        search.mode,
        search.requested_mode,
        search.source,
        search.complete_only,
        search.results,
        search.latency_ms,
        search.degraded,
        search.generation,
    )
    .fetch_one(&mut **tx)
    .await
}

/// Records a click under the session and arm of the search it answers. Returns how many rows
/// were written: 0 when the search is unknown.
pub async fn insert_click(
    tx: &mut Transaction<'_, Postgres>,
    query_id: &str,
    document_id: &str,
    rank: i32,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"
        INSERT INTO search_events (kind, session_id, experiment, variant, query_id, document_id, rank)
        SELECT 'click', session_id, experiment, variant, query_id, $2, $3
        FROM search_events WHERE kind = 'search' AND query_id = ($1::text)::uuid
        "#,
        query_id,
        document_id,
        rank,
    )
    .execute(&mut **tx)
    .await?;
    Ok(done.rows_affected())
}

pub async fn read(
    pool: &PgPool,
    experiment: &str,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as!(
        EventRow,
        r#"
        SELECT at, kind, session_id, variant, query_id::text AS "query_id!", query, results,
               latency_ms, degraded, document_id, rank
        FROM search_events
        WHERE experiment = $1 AND ($2::timestamptz IS NULL OR at >= $2)
        ORDER BY at, id
        "#,
        experiment,
        since,
    )
    .fetch_all(pool)
    .await
}
