use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::graph::{Graph, GraphQuery, GraphRebuild},
    use_cases::graph,
};
use axum::{
    Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/graph", get(neighbors))
        .route("/graph/rebuild", post(rebuild))
}

/// The documents a document links to and the documents that link to it (one hop), with the
/// linking URLs as evidence. A link means a link, nothing more: not support, not influence.
#[utoipa::path(
    get,
    path = "/api/graph",
    params(GraphQuery),
    responses(
        (status = 200, body = Graph),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
)]
pub async fn neighbors(
    State(state): State<AppState>,
    query: Result<Query<GraphQuery>, QueryRejection>,
) -> Result<Json<Graph>, Error> {
    let Query(query) =
        query.map_err(|_| Error(StatusCode::BAD_REQUEST, "Invalid graph parameters".into()))?;
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let graph = graph::neighbors::execute(&mut tx, query).await?;
    tx.commit().await?;
    Ok(Json(graph))
}

/// Rebuilds the reference edges from the stored links. Reads only the database.
#[utoipa::path(
    post,
    path = "/api/graph/rebuild",
    responses((status = 200, body = GraphRebuild), (status = 503, body = ErrorBody)),
)]
pub async fn rebuild(State(state): State<AppState>) -> Result<Json<GraphRebuild>, Error> {
    let mut tx = state.pool.begin().await?;
    let result = graph::rebuild::execute(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
