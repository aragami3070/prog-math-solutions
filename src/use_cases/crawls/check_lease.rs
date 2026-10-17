use super::errors::Error;
use crate::repository::health;
use sqlx::{Postgres, Transaction};

// Runs on the dedicated session holding the crawl lock. A lost connection
// aborts the worker before it sends another external request.
pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    health::check(tx).await?;
    Ok(())
}
