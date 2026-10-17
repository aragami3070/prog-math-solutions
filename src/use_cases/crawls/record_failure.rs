use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    error: &str,
) -> Result<(), Error> {
    crawls::failed(tx, run_id, error).await?;
    Ok(())
}
