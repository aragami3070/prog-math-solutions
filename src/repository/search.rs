use crate::models::search::IndexRow;
use futures_util::stream::BoxStream;
use sqlx::{Postgres, Transaction};

/// Every document, streamed so the corpus is never held in memory at once.
pub fn documents<'a>(
    tx: &'a mut Transaction<'_, Postgres>,
) -> BoxStream<'a, Result<IndexRow, sqlx::Error>> {
    sqlx::query_as!(
        IndexRow,
        r#"
        SELECT
            id, source, native_id, canonical_url, title, body_text,
            completeness AS "completeness!",
            (SELECT EXTRACT(YEAR FROM p.published_at AT TIME ZONE 'UTC')::INT
             FROM posts AS p WHERE p.document_id = documents.id AND p.post_number = 1) AS year
        FROM documents
        ORDER BY id
        "#,
    )
    .fetch(&mut **tx)
}

/// What the index must reflect: changes when a document appears, goes, or changes content or
/// completeness. Not affected by a recrawl that found the same content.
pub async fn corpus_marker(tx: &mut Transaction<'_, Postgres>) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        SELECT COALESCE(md5(string_agg(
            source || ':' || native_id || ':' || content_hash || ':' || completeness || ':' || title,
            ',' ORDER BY id)), '') || ':' || COUNT(*)::TEXT AS "marker!"
        FROM documents
        "#
    )
    .fetch_one(&mut **tx)
    .await
}
