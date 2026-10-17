use super::errors::Error;
use sqlx::{Postgres, Transaction};

pub async fn requests(tx: &mut Transaction<'_, Postgres>) -> Result<(i64, i64), Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) AS "document_requests!",
            COUNT(*) FILTER (
                WHERE status IS NULL OR status >= 400 OR error IS NOT NULL
            ) AS "http_errors!"
        FROM fetches
        WHERE kind = 'document'
        "#,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((row.document_requests, row.http_errors))
}
