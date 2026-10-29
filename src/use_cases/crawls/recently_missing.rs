use super::errors::Error;
use crate::repository::crawls;
use sqlx::{Postgres, Transaction};
use std::time::Duration;

/// A page an earlier run found missing is not requested again until the reuse window passes.
/// Disabled together with page reuse (zero window).
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    url: &str,
    window: Duration,
) -> Result<bool, Error> {
    if window.is_zero() {
        return Ok(false);
    }
    Ok(crawls::recently_missing(tx, url, window.as_secs_f64()).await?)
}
