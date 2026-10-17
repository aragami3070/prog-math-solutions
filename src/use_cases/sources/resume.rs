use super::errors::Error;
use crate::{models::sources::Source, repository::sources};
use sqlx::{Postgres, Transaction};

pub async fn execute(tx: &mut Transaction<'_, Postgres>, source: Source) -> Result<(), Error> {
    sources::resume(tx, source).await?;
    Ok(())
}
