use super::errors::Error;
use crate::{
    models::search::RebuildResult,
    repository::search,
    search::{IndexDocument, SearchHandle, display_title},
};
use futures_util::TryStreamExt;
use sqlx::{Postgres, Transaction};
use std::time::Instant;

/// Builds a new generation from one consistent database snapshot and makes it current.
/// The old generation keeps serving until the new one is complete. No requests to sources.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    handle: &SearchHandle,
) -> Result<RebuildResult, Error> {
    let _only_builder = handle.rebuilding.try_lock().map_err(|_| Error::Busy)?;
    let started = Instant::now();
    let mut builder = handle.begin()?;
    builder.set_marker(search::corpus_marker(tx).await?);
    let mut rows = search::documents(tx);
    while let Some(row) = rows.try_next().await? {
        let title = display_title(&row.source, &row.title);
        builder.add(IndexDocument {
            id: row.id,
            source: row.source,
            native_id: row.native_id,
            url: row.canonical_url,
            title,
            body: row.body_text,
            completeness: row.completeness,
        })?;
    }
    drop(rows);
    let index = tokio::task::spawn_blocking(move || builder.finish()).await??;
    let index = handle.install(index)?;
    Ok(RebuildResult {
        generation: index.generation().to_owned(),
        documents: index.documents(),
        took_ms: started.elapsed().as_millis() as u64,
    })
}
