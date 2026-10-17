use super::{errors::Error, save_document};
use crate::{
    models::{crawls::CollectedPage, sources::Source},
    repository::crawls,
};
use sqlx::{Postgres, Transaction};

// Document, discovered links and completion form a single checkpoint.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    source: Source,
    page: CollectedPage,
) -> Result<(), Error> {
    if let Some(doc) = page.document {
        save_document::execute(tx, run_id, source, page.fetch_id, doc).await?;
    } else {
        crawls::succeeded(tx, run_id).await?;
    }
    crawls::enqueue(tx, run_id, &page.links).await?;
    crawls::finish_page(tx, run_id, page.id, None).await?;
    Ok(())
}
