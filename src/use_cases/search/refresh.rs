use super::{errors::Error, rebuild};
use crate::{models::search::RebuildResult, repository::search, search::SearchHandle};
use sqlx::PgPool;

/// Rebuilds the index only if the corpus changed since the current generation was built.
/// Returns `None` when the index is already current. Reads only the database.
pub async fn execute(pool: &PgPool, handle: &SearchHandle) -> Result<Option<RebuildResult>, Error> {
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let marker = search::corpus_marker(&mut tx).await?;
    if handle
        .current()
        .is_some_and(|index| index.marker() == Some(marker.as_str()))
    {
        return Ok(None);
    }
    Ok(Some(rebuild::execute(&mut tx, handle).await?))
}
