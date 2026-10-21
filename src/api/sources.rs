use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::{
        exports::{ExportDocument, ExportQuery},
        pagination::Pagination,
        sources::{DocumentPage, Source},
    },
    use_cases::sources,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents", get(documents))
        .route("/sources/export", get(export))
        .route("/sources/{source}/resume", post(resume))
}

/// Download all matching documents from one consistent database snapshot.
/// ZIP contains JSONL, JSON, CSV and a checksummed manifest. No source HTTP requests.
#[utoipa::path(
    get,
    path = "/api/sources/export",
    params(ExportQuery),
    responses(
        (
            status = 200,
            description = "Corpus attachment with schema, count, snapshot and SHA-256 headers",
            content(
                ([ExportDocument] = "application/json"),
                (String = "application/x-ndjson"),
                (String = "text/csv"),
                ([u8] = "application/zip"),
            ),
        ),
        (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody),
        (status = 500, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn export(
    State(state): State<AppState>,
    query: Result<Query<ExportQuery>, QueryRejection>,
) -> Result<Response, Error> {
    let Query(query) = query.map_err(|_| {
        Error(
            StatusCode::BAD_REQUEST,
            "Invalid corpus export parameters".into(),
        )
    })?;
    let _permit = state.export_slots.try_acquire().map_err(|_| {
        Error(
            StatusCode::CONFLICT,
            "Another export is being generated; try again later".into(),
        )
    })?;
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let artifact = sources::export_corpus::execute(&mut tx, &query).await?;
    tx.commit().await?;
    let headers = [
        (
            header::CONTENT_TYPE.as_str(),
            query.format.content_type().to_owned(),
        ),
        (
            header::CONTENT_DISPOSITION.as_str(),
            format!(
                "attachment; filename=\"rust-knowledge.{}\"",
                query.format.extension()
            ),
        ),
        (header::CONTENT_LENGTH.as_str(), artifact.size.to_string()),
        (header::CACHE_CONTROL.as_str(), "no-store".to_owned()),
        ("x-content-type-options", "nosniff".to_owned()),
        ("x-corpus-schema-version", "1".to_owned()),
        ("x-corpus-documents", artifact.count.to_string()),
        ("x-corpus-snapshot-at", artifact.snapshot_at.to_rfc3339()),
        ("x-content-sha256", artifact.sha256),
    ];
    Ok((
        headers,
        Body::from_stream(tokio_util::io::ReaderStream::new(artifact.file)),
    )
        .into_response())
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
