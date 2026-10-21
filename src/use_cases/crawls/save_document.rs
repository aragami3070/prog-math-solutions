use super::errors::Error;
use crate::{
    models::sources::{ParsedPage, Source},
    repository::crawls,
    use_cases::sources::save_page,
};
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    source: Source,
    fetch_id: i64,
    page: &ParsedPage,
) -> Result<(), Error> {
    save_page::execute(tx, source, fetch_id, page).await?;
    crawls::succeeded(tx, run_id).await?;
    Ok(())
}
