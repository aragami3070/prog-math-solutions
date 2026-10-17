use super::errors::Error;
use crate::models::{
    pagination::Pagination,
    sources::{DocumentSummary, ParsedDocument, Source},
    statistics::SourceStats,
};
use sqlx::{Postgres, Transaction};

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
            completeness,
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

pub async fn upsert_document(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    fetch_id: i64,
    doc: ParsedDocument,
    hash: &str,
) -> Result<(), Error> {
    sqlx::query!(
        r#"
        INSERT INTO documents (
            source, native_id, canonical_url, title, body_text,
            content_hash, completeness, completeness_reason,
            metadata, parser_version, fetch_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'html-v1', $10)
        ON CONFLICT (source, native_id) DO UPDATE SET
            canonical_url = EXCLUDED.canonical_url,
            title = EXCLUDED.title,
            body_text = EXCLUDED.body_text,
            content_hash = EXCLUDED.content_hash,
            completeness = EXCLUDED.completeness,
            completeness_reason = EXCLUDED.completeness_reason,
            metadata = EXCLUDED.metadata,
            parser_version = EXCLUDED.parser_version,
            fetch_id = EXCLUDED.fetch_id,
            last_seen_at = NOW()
        "#,
        source.as_str(),
        doc.native_id,
        doc.canonical_url,
        doc.title,
        doc.body_text,
        hash,
        doc.completeness,
        doc.completeness_reason,
        doc.metadata,
        fetch_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn document_metadata(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    native_id: &str,
) -> Result<Option<serde_json::Value>, Error> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT metadata FROM documents
        WHERE source = $1 AND native_id = $2
        "#,
        source.as_str(),
        native_id,
    )
    .fetch_optional(&mut **tx)
    .await?)
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
