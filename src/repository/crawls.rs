use super::errors::Error;
use crate::models::{
    crawls::{CrawlRun, FetchRecord, QueueLink, QueuedPage, RunStatus},
    sources::Source,
};
use sqlx::{Postgres, Transaction};

pub async fn try_lock(tx: &mut Transaction<'_, Postgres>) -> Result<bool, Error> {
    Ok(
        sqlx::query_scalar!(r#" SELECT pg_try_advisory_lock(736251094) AS "locked!" "#,)
            .fetch_one(&mut **tx)
            .await?,
    )
}

pub async fn recover_interrupted(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_runs
        SET
            status = 'interrupted',
            finished_at = NOW(),
            error = 'collector process interrupted'
        WHERE status = 'running'
        "#,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    requested: usize,
) -> Result<CrawlRun, Error> {
    Ok(sqlx::query_as!(
        CrawlRun,
        r#"
        INSERT INTO crawl_runs (source, status, requested)
        VALUES ($1, 'running', $2)
        RETURNING
            id, source, status AS "status: RunStatus", requested, succeeded, failed,
            started_at, finished_at, error
        "#,
        source.as_str(),
        requested as i32,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn record_fetch(
    tx: &mut Transaction<'_, Postgres>,
    fetch: FetchRecord<'_>,
) -> Result<i64, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO fetches (run_id, url, kind, status, content_type, body, error)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        fetch.run_id,
        fetch.url,
        fetch.kind.as_str(),
        fetch.status.map(i32::from),
        fetch.content_type,
        fetch.body,
        fetch.error,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn succeeded(tx: &mut Transaction<'_, Postgres>, run_id: i64) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_runs
        SET succeeded = succeeded + 1
        WHERE id = $1
        "#,
        run_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn failed(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    error: &str,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_runs
        SET failed = failed + 1, error = $2
        WHERE id = $1
        "#,
        run_id,
        error,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    status: RunStatus,
    error: Option<&str>,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_runs
        SET
            status = $2,
            finished_at = NOW(),
            error = COALESCE($3, error)
        WHERE id = $1
        "#,
        run_id,
        status.as_str(),
        error,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn recent(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<CrawlRun>, Error> {
    Ok(sqlx::query_as!(
        CrawlRun,
        r#"
        SELECT
            id, source, status AS "status: RunStatus", requested, succeeded, failed,
            started_at, finished_at, error
        FROM crawl_runs
        ORDER BY id DESC
        LIMIT 10
        "#,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    links: &[QueueLink],
) -> Result<(), Error> {
    let urls: Vec<String> = links.iter().map(|l| l.url.clone()).collect();
    let fetch_urls: Vec<String> = links.iter().map(|l| l.fetch_url.clone()).collect();
    sqlx::query!(
        r#"
        WITH added AS (
            INSERT INTO crawl_queue (run_id, url, fetch_url)
            SELECT $1, t.url, t.fetch_url
            FROM UNNEST($2::TEXT[], $3::TEXT[]) AS t(url, fetch_url)
            ON CONFLICT (run_id, url) DO NOTHING
            RETURNING id
        )
        UPDATE crawl_runs
        SET requested = requested + (SELECT COUNT(*)::INTEGER FROM added)
        WHERE id = $1
        "#,
        run_id,
        &urls,
        &fetch_urls,
    )
    .execute(&mut **tx)
    .await?;
    // A page first seen through a slug-less link learns its slug from a later sighting.
    sqlx::query!(
        r#"
        UPDATE crawl_queue AS q
        SET fetch_url = t.fetch_url
        FROM UNNEST($2::TEXT[], $3::TEXT[]) AS t(url, fetch_url)
        WHERE q.run_id = $1
          AND q.url = t.url
          AND q.status = 'pending'
          AND q.fetch_url = q.url
          AND t.fetch_url <> t.url
        "#,
        run_id,
        &urls,
        &fetch_urls,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn next_page(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
) -> Result<Option<QueuedPage>, Error> {
    Ok(sqlx::query_as!(
        QueuedPage,
        r#"
        SELECT id, url, fetch_url FROM crawl_queue
        WHERE run_id = $1 AND status = 'pending'
        ORDER BY id LIMIT 1
        "#,
        run_id,
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn finish_page(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    page_id: i64,
    error: Option<&str>,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_queue
        SET status = CASE WHEN $3::TEXT IS NULL THEN 'done' ELSE 'failed' END,
            error = $3
        WHERE id = $2 AND run_id = $1
        "#,
        run_id,
        page_id,
        error,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn resumable(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
) -> Result<Option<CrawlRun>, Error> {
    Ok(sqlx::query_as!(
        CrawlRun,
        r#"
        SELECT id, source, status AS "status: RunStatus", requested, succeeded, failed,
               started_at, finished_at, error
        FROM crawl_runs AS r
        WHERE source = $1 AND status IN ('failed', 'interrupted', 'blocked')
          AND EXISTS (SELECT 1 FROM crawl_queue WHERE run_id = r.id AND status <> 'done')
        ORDER BY id DESC LIMIT 1
        "#,
        source.as_str(),
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn reset_failed_pages(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_queue SET status = 'pending', error = NULL
        WHERE run_id = $1 AND status = 'failed'
        "#,
        run_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn restart(tx: &mut Transaction<'_, Postgres>, run_id: i64) -> Result<CrawlRun, Error> {
    Ok(sqlx::query_as!(
        CrawlRun,
        r#"
        UPDATE crawl_runs
        SET status = 'running', finished_at = NULL, error = NULL, failed = 0
        WHERE id = $1
        RETURNING id, source, status AS "status: RunStatus", requested, succeeded, failed,
                  started_at, finished_at, error
        "#,
        run_id,
    )
    .fetch_one(&mut **tx)
    .await?)
}
