use super::errors::Error;
use crate::{models::crawls::FetchRecord, repository::crawls};
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    fetch: FetchRecord<'_>,
) -> Result<i64, Error> {
    Ok(crawls::record_fetch(tx, fetch).await?)
}
