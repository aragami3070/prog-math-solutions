use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};

pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    if crawls::try_lock(tx).await? {
        crawls::recover_interrupted(tx).await?;
    }
    Ok(())
}
