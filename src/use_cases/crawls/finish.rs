use super::errors::Error;
use crate::{
    models::{crawls::RunStatus, sources::Source},
    repository::{crawls, sources},
};
use sqlx::{Postgres, Transaction};

// Persist the source pause and terminal run state atomically.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    source: Source,
    status: RunStatus,
    error: Option<&str>,
) -> Result<(), Error> {
    if status == RunStatus::Blocked {
        sources::pause(tx, source, error.unwrap_or("source blocked")).await?;
    }
    crawls::finish(tx, run_id, status, error).await?;
    Ok(())
}
