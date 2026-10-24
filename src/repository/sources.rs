use super::errors::Error;
use crate::models::{
    exports::{ExportLink, ExportPage, ExportPost, ExportRow},
    pagination::Pagination,
    sources::{Completeness, DocumentSummary, ParsedLink, ParsedPage, ParsedPost, Source},
    statistics::SourceStats,
};
use sqlx::{Postgres, Transaction};

pub async fn export_snapshot_time(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<chrono::DateTime<chrono::Utc>, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT transaction_timestamp() AS "snapshot_at!"
        "#,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn export_batch(
    tx: &mut Transaction<'_, Postgres>,
    source: Option<Source>,
    complete_only: bool,
    after: &str,
) -> Result<Vec<ExportRow>, Error> {
    Ok(sqlx::query_as!(
        ExportRow,
        r#"
        SELECT
            d.id, d.source, d.native_id, d.canonical_url, d.title, d.body_text,
            d.content_hash, d.completeness AS "completeness: Completeness",
            d.completeness_reason, d.parser_version,
            COALESCE(
                (SELECT MAX(p.fetched_at) FROM document_pages AS p WHERE p.document_id = d.id),
                d.last_seen_at
            ) AS "fetched_at!",
            d.first_seen_at, d.last_seen_at
        FROM documents AS d
        WHERE ($1::TEXT IS NULL OR d.source = $1)
          AND (NOT $2 OR d.completeness = 'complete')
          AND (d.source || ':' || d.native_id) COLLATE "C" > $3 COLLATE "C"
        ORDER BY (d.source || ':' || d.native_id) COLLATE "C"
        LIMIT 32
        "#,
        source.map(Source::as_str),
        complete_only,
        after,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn export_posts(
    tx: &mut Transaction<'_, Postgres>,
    document_ids: &[i64],
) -> Result<Vec<ExportPost>, Error> {
    Ok(sqlx::query_as!(
        ExportPost,
        r#"
        SELECT document_id, post_number, author, published_at, text, code
        FROM posts
        WHERE document_id = ANY($1)
        ORDER BY document_id, post_number
        "#,
        document_ids,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn export_links(
    tx: &mut Transaction<'_, Postgres>,
    document_ids: &[i64],
) -> Result<Vec<ExportLink>, Error> {
    Ok(sqlx::query_as!(
        ExportLink,
        r#"
        SELECT l.document_id, p.post_number, l.target_url, l.anchor_text
        FROM links AS l
        JOIN posts AS p ON p.id = l.post_id
        WHERE l.document_id = ANY($1)
        ORDER BY l.document_id, p.post_number, l.id
        "#,
        document_ids,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn export_pages(
    tx: &mut Transaction<'_, Postgres>,
    document_ids: &[i64],
) -> Result<Vec<ExportPage>, Error> {
    Ok(sqlx::query_as!(
        ExportPage,
        r#"
        SELECT document_id, page_no, url, fetch_id
        FROM document_pages
        WHERE document_id = ANY($1)
        ORDER BY document_id, page_no
        "#,
        document_ids,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn count_documents(tx: &mut Transaction<'_, Postgres>) -> Result<i64, Error> {
    Ok(sqlx::query_scalar!(
        r#"
            SELECT COUNT(*) AS "count!"
            FROM documents
            "#,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn documents(
    tx: &mut Transaction<'_, Postgres>,
    pagination: &Pagination,
) -> Result<Vec<DocumentSummary>, Error> {
    Ok(sqlx::query_as!(
        DocumentSummary,
        r#"
        SELECT
            id,
            source,
            native_id,
            canonical_url,
            title,
            LEFT(body_text, 320) AS "excerpt!",
            CHAR_LENGTH(body_text)::BIGINT AS "text_chars!",
            completeness AS "completeness: Completeness",
            completeness_reason,
            first_seen_at,
            last_seen_at
        FROM documents
        ORDER BY first_seen_at DESC, id DESC
        LIMIT $1 OFFSET $2
        "#,
        i64::from(pagination.page_size),
        pagination.offset(),
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn state(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
) -> Result<(bool, Option<String>), Error> {
    let row = sqlx::query!(
        r#"
        SELECT paused, pause_reason
        FROM crawl_sources
        WHERE source = $1
        "#,
        source.as_str(),
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((row.paused, row.pause_reason))
}

pub async fn resume(tx: &mut Transaction<'_, Postgres>, source: Source) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_sources
        SET paused = FALSE, pause_reason = NULL
        WHERE source = $1
        "#,
        source.as_str(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn pause(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    reason: &str,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE crawl_sources
        SET paused = TRUE, pause_reason = $2
        WHERE source = $1
        "#,
        source.as_str(),
        reason,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Creates or updates the document shell. Text and completeness are derived
/// afterwards by `refresh_document`, so an existing body is never blanked here.
pub async fn upsert_document(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    page: &ParsedPage,
    fetch_id: i64,
    parser_version: &str,
) -> Result<i64, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO documents (
            source, native_id, canonical_url, title, body_text,
            content_hash, completeness, completeness_reason,
            metadata, parser_version
        )
        VALUES ($1, $2, $3, $4, '', '', 'unknown', '', $5, $6)
        ON CONFLICT (source, native_id) DO UPDATE SET
            canonical_url = EXCLUDED.canonical_url,
            title = EXCLUDED.title,
            metadata = EXCLUDED.metadata,
            parser_version = EXCLUDED.parser_version,
            last_seen_at = GREATEST(
                documents.last_seen_at,
                (SELECT fetched_at FROM fetches WHERE id = $7)
            )
        RETURNING id
        "#,
        source.as_str(),
        page.native_id,
        page.canonical_url,
        page.title,
        serde_json::json!({"license_source": source.license_source()}),
        parser_version,
        fetch_id,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn upsert_page(
    tx: &mut Transaction<'_, Postgres>,
    document_id: i64,
    page: &ParsedPage,
    fetch_id: i64,
) -> Result<i64, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO document_pages (document_id, page_no, url, fetch_id, has_next, fetched_at)
        VALUES ($1, $2, $3, $4, $5, (SELECT fetched_at FROM fetches WHERE id = $4))
        ON CONFLICT (document_id, page_no) DO UPDATE SET
            url = EXCLUDED.url,
            fetch_id = EXCLUDED.fetch_id,
            has_next = EXCLUDED.has_next,
            fetched_at = EXCLUDED.fetched_at
        RETURNING id
        "#,
        document_id,
        page.page_no,
        page.page_url,
        fetch_id,
        page.has_next,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn delete_page_posts(
    tx: &mut Transaction<'_, Postgres>,
    page_id: i64,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        DELETE FROM posts
        WHERE page_id = $1
        "#,
        page_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A post that moved to another page (after deletions shifted pagination) is
/// reassigned instead of duplicated.
pub async fn upsert_post(
    tx: &mut Transaction<'_, Postgres>,
    document_id: i64,
    page_id: i64,
    post: &ParsedPost,
) -> Result<i64, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO posts (document_id, page_id, post_number, author, published_at, text, code)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (document_id, post_number) DO UPDATE SET
            page_id = EXCLUDED.page_id,
            author = EXCLUDED.author,
            published_at = EXCLUDED.published_at,
            text = EXCLUDED.text,
            code = EXCLUDED.code
        RETURNING id
        "#,
        document_id,
        page_id,
        post.post_number,
        post.author,
        post.published_at,
        post.text,
        serde_json::json!(post.code),
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn replace_post_links(
    tx: &mut Transaction<'_, Postgres>,
    document_id: i64,
    post_id: i64,
    links: &[ParsedLink],
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        DELETE FROM links
        WHERE post_id = $1
        "#,
        post_id,
    )
    .execute(&mut **tx)
    .await?;
    let targets: Vec<String> = links.iter().map(|l| l.target_url.clone()).collect();
    let anchors: Vec<String> = links
        .iter()
        .map(|l| l.anchor_text.clone().unwrap_or_default())
        .collect();
    sqlx::query!(
        r#"
        INSERT INTO links (document_id, post_id, target_url, anchor_text)
        SELECT $1, $2, t.target, NULLIF(t.anchor, '')
        FROM UNNEST($3::TEXT[], $4::TEXT[]) AS t(target, anchor)
        ON CONFLICT (post_id, target_url) DO NOTHING
        "#,
        document_id,
        post_id,
        &targets,
        &anchors,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn page_states(
    tx: &mut Transaction<'_, Postgres>,
    document_id: i64,
) -> Result<Vec<(i32, bool)>, Error> {
    let rows = sqlx::query!(
        r#"
        SELECT page_no, has_next
        FROM document_pages
        WHERE document_id = $1
        "#,
        document_id,
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().map(|r| (r.page_no, r.has_next)).collect())
}

/// Rebuilds the derived text, hash and completeness from the stored posts and pages.
pub async fn refresh_document(
    tx: &mut Transaction<'_, Postgres>,
    document_id: i64,
    completeness: Completeness,
    reason: &str,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        UPDATE documents AS d
        SET
            body_text = agg.body,
            content_hash = encode(sha256(convert_to(agg.body, 'UTF8')), 'hex'),
            completeness = $2,
            completeness_reason = $3
        FROM (
            SELECT COALESCE(string_agg(text, E'\n\n' ORDER BY post_number), '') AS body
            FROM posts
            WHERE document_id = $1
        ) AS agg
        WHERE d.id = $1
        "#,
        document_id,
        completeness.as_str(),
        reason,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn statistics(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<SourceStats>, Error> {
    Ok(sqlx::query_as!(
        SourceStats,
        r#"
        SELECT
            s.source AS "source!",
            COUNT(d.id) AS "documents!",
            COUNT(d.id) FILTER (WHERE d.completeness = 'complete') AS "complete!",
            COUNT(d.id) FILTER (WHERE d.completeness = 'partial') AS "partial!",
            COUNT(d.id) FILTER (WHERE d.completeness = 'unknown') AS "unknown!",
            COALESCE(SUM(CHAR_LENGTH(d.body_text)), 0)::BIGINT AS "text_chars!",
            MAX(d.last_seen_at) AS "last_collected_at?",
            s.paused AS "paused!",
            s.pause_reason
        FROM crawl_sources AS s
        LEFT JOIN documents AS d ON d.source = s.source
        GROUP BY s.source, s.paused, s.pause_reason
        ORDER BY s.source
        "#,
    )
    .fetch_all(&mut **tx)
    .await?)
}
