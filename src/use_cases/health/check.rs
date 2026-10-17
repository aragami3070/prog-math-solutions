use super::errors::Error;
use crate::repository::health;
use sqlx::{Postgres, Transaction};

pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    health::check(tx).await?;
    Ok(())
}
