use super::errors::Error;
use sqlx::{Postgres, Transaction};

pub async fn check(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    sqlx::query!("SELECT 1 AS healthy")
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}
