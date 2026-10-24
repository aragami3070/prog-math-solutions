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
            completeness AS "completeness!"
        FROM documents
        ORDER BY id
        "#,
    )
    .fetch(&mut **tx)
}
