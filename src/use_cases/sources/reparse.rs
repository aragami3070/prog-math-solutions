use super::{errors::Error, save_page};
use crate::{
    crawler::{discovery, sources::parse},
    models::sources::Source,
    repository::sources,
};
use sqlx::{Postgres, Transaction};
use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Saved,
    /// Navigation pages and fetches without a stored body carry no document.
    Skipped,
}

/// Fetch IDs to replay, oldest first, so the newest fetch of a page wins.
pub async fn fetch_ids(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<i64>, Error> {
    Ok(sources::reparse_fetch_ids(tx).await?)
}

/// Rebuilds pages, posts and links from a stored response. No requests to source websites.
pub async fn execute(tx: &mut Transaction<'_, Postgres>, fetch_id: i64) -> Result<Outcome, Error> {
    let Some(fetch) = sources::stored_fetch(tx, fetch_id).await? else {
        return Ok(Outcome::Skipped);
    };
    let parse_error = |reason: String| Error::Reparse {
        fetch_id,
        url: fetch.url.clone(),
        reason,
    };
    let url = Url::parse(&fetch.url).map_err(|e| parse_error(e.to_string()))?;
    let Some(source) = Source::from_url(&url) else {
        return Ok(Outcome::Skipped);
    };
    let normalized =
        discovery::normalize(source, &fetch.url).map_err(|e| parse_error(e.to_string()))?;
    if !discovery::is_document(source, &normalized) {
        return Ok(Outcome::Skipped);
    }
    let page = parse(source, &fetch.url, &fetch.body).map_err(|e| parse_error(e.to_string()))?;
    save_page::execute(tx, source, fetch_id, &page).await?;
    Ok(Outcome::Saved)
}
