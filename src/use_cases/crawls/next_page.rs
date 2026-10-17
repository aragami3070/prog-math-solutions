use super::errors::Error;
use crate::{models::crawls::QueuedPage, repository::crawls};
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
) -> Result<Option<QueuedPage>, Error> {
    Ok(crawls::next_page(tx, run_id).await?)
}
