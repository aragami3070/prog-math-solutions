use super::errors::Error;
use crate::{
    models::statistics::Statistics,
    repository::{crawls, sources, statistics},
};
use sqlx::{Postgres, Transaction};

pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<Statistics, Error> {
    let sources = sources::statistics(tx).await?;
    let (document_requests, http_errors) = statistics::requests(tx).await?;
    let recent_runs = crawls::recent(tx).await?;
    Ok(Statistics {
        total_documents: sources.iter().map(|s| s.documents).sum(),
        document_requests,
        http_errors,
        sources,
        recent_runs,
    })
}
