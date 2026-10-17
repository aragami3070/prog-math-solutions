use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::{
        pagination::Pagination,
        sources::{DocumentPage, Source},
    },
    use_cases::sources,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents", get(documents))
        .route("/sources/{source}/resume", post(resume))
}

#[utoipa::path(
    get,
    path = "/api/documents",
    params(Pagination),
    responses(
        (status = 200, body = DocumentPage),
        (status = 400, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn documents(
    State(state): State<AppState>,
    query: Result<Query<Pagination>, QueryRejection>,
) -> Result<Json<DocumentPage>, Error> {
    let Query(pagination) = query.map_err(|_| {
        Error(
            StatusCode::BAD_REQUEST,
            "Invalid pagination parameters".into(),
        )
    })?;
    sources::list_documents::validate(&pagination)?;
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let result = sources::list_documents::execute(&mut tx, pagination).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[utoipa::path(
    post,
    path = "/api/sources/{source}/resume",
    params(("source" = Source, Path)),
    responses(
        (status = 204, description = "Source resumed after manual review"),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn resume(
    State(state): State<AppState>,
    Path(source): Path<Source>,
) -> Result<StatusCode, Error> {
    let mut tx = state.pool.begin().await?;
    sources::resume::execute(&mut tx, source).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
