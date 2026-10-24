use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::search::{RebuildResult, SearchQuery, SearchResults},
    use_cases::search,
};
use axum::{
    Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search))
        .route("/search/rebuild", post(rebuild))
}

/// Lexical search over the collected documents (BM25 on title and body, exact IDs,
/// phrases, filters). The response says which index generation answered.
#[utoipa::path(
    get,
    path = "/api/search",
    params(SearchQuery),
    responses(
        (status = 200, body = SearchResults),
        (status = 400, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn search(
    State(state): State<AppState>,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> Result<Json<SearchResults>, Error> {
    let Query(query) =
        query.map_err(|_| Error(StatusCode::BAD_REQUEST, "Invalid search parameters".into()))?;
    Ok(Json(search::query::execute(&state.search, query).await?))
}

/// Rebuilds the search index from the database. Reads only the database, never the sources.
#[utoipa::path(
    post,
    path = "/api/search/rebuild",
    responses(
        (status = 200, body = RebuildResult),
        (status = 409, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn rebuild(State(state): State<AppState>) -> Result<Json<RebuildResult>, Error> {
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let result = search::rebuild::execute(&mut tx, &state.search).await?;
    tx.commit().await?;
    Ok(Json(result))
}
