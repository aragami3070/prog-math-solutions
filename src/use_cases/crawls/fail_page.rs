use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    page_id: i64,
    error: &str,
    links: &[String],
) -> Result<(), Error> {
    crawls::enqueue(tx, run_id, links).await?;
    crawls::finish_page(tx, run_id, page_id, Some(error)).await?;
    crawls::failed(tx, run_id, error).await?;
    Ok(())
}
