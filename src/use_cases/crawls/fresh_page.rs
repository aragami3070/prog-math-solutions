use super::errors::Error;
use crate::{crawler::sources::PARSER_VERSION, models::crawls::StoredPage, repository::crawls};
use sqlx::{Postgres, Transaction};
use std::time::Duration;

/// A recent stored response for this page URL, if reuse is enabled. No request to the source.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    url: &str,
    fresh_for: Duration,
) -> Result<Option<StoredPage>, Error> {
    if fresh_for.is_zero() {
        return Ok(None);
    }
    Ok(crawls::fresh_page(tx, url, fresh_for.as_secs_f64(), PARSER_VERSION).await?)
}
