use crate::{
    experiment::EXPERIMENT,
    models::{events::NewSearch, search::SearchResults},
    repository,
};
use sqlx::{Postgres, Transaction};

/// Records the search under the arm it was answered by and returns the `query_id` for clicks.
/// Never fails the search: a lost event is logged and the answer goes out without a query id.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    session: &str,
    variant: &str,
    results: &SearchResults,
) -> Option<String> {
    let search = NewSearch {
        session_id: session,
        query: &results.query,
        mode: results.scope.mode,
        requested_mode: results.scope.requested_mode,
        source: results.scope.source.as_deref(),
        complete_only: results.scope.complete_only,
        results: results.total.min(i32::MAX as usize) as i32,
        latency_ms: results.took_ms.min(i32::MAX as u64) as i32,
        degraded: results.scope.degraded.is_some(),
        generation: &results.scope.generation,
    };
    match repository::events::insert_search(tx, EXPERIMENT.id, variant, &search).await {
        Ok(id) => Some(id),
        Err(error) => {
            tracing::warn!(%error, "could not record the search event");
            None
        }
    }
}
