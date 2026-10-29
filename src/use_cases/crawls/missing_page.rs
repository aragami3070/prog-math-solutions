use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    page_id: i64,
    reason: &str,
) -> Result<(), Error> {
    crawls::missing_page(tx, run_id, page_id, reason).await?;
    Ok(())
}
