use super::errors::Error;
use crate::{models::graph::GraphRebuild, repository::graph};
use sqlx::{Postgres, Transaction};
use std::time::Instant;

/// Rebuilds the reference edges from the stored links; reads only the database.
pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<GraphRebuild, Error> {
    let started = Instant::now();
    let edges = graph::rebuild_references(tx).await?;
    Ok(GraphRebuild {
        edges,
        took_ms: started.elapsed().as_millis() as u64,
    })
}
