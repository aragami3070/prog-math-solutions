use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};

// Session-level lock: it stays held by the caller's connection after commit.
pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<(), Error> {
    if !crawls::try_lock(tx).await? {
        return Err(Error::Busy);
    }
    Ok(())
}
