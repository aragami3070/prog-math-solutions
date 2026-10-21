use super::errors::Error;
use crate::{
    crawler::sources::PARSER_VERSION,
    models::sources::{Completeness, ParsedPage, Source},
    repository::sources,
};
use sqlx::{Postgres, Transaction};

/// Stores one fetched page and rebuilds the document derived from all its pages.
/// Only this page and its posts are replaced; other pages of the document stay.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    source: Source,
    fetch_id: i64,
    page: &ParsedPage,
) -> Result<(), Error> {
    let document_id = sources::upsert_document(tx, source, page, fetch_id, PARSER_VERSION).await?;
    let page_id = sources::upsert_page(tx, document_id, page, fetch_id).await?;
    sources::delete_page_posts(tx, page_id).await?;
    for post in &page.posts {
        let post_id = sources::upsert_post(tx, document_id, page_id, post).await?;
        sources::replace_post_links(tx, document_id, post_id, &post.links).await?;
    }
    let states = sources::page_states(tx, document_id).await?;
    let (completeness, reason) = Completeness::evaluate(&states);
    sources::refresh_document(tx, document_id, completeness, &reason).await?;
    Ok(())
}
