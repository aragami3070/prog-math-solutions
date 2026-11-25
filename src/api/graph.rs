use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::graph::{Graph, GraphOverview, GraphQuery, GraphRebuild},
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
        .route("/graph/overview", get(overview))
        .route("/graph/related", get(related))
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

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[serde(deny_unknown_fields)]
pub struct RelatedQuery {
    document: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct RelatedView {
    topics: Vec<crate::search::related::TopicCandidate>,
    similar: Option<Vec<crate::search::related::SimilarCandidate>>,
    generation: String,
}

/// Experimental keyword topic relations and unreviewed offline similarity candidates.
#[utoipa::path(get, path = "/api/graph/related", params(RelatedQuery),
    responses((status = 200, body = RelatedView), (status = 404, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn related(
    State(state): State<AppState>,
    query: Result<Query<RelatedQuery>, QueryRejection>,
) -> Result<Json<RelatedView>, Error> {
    let Query(query) =
        query.map_err(|_| Error(StatusCode::BAD_REQUEST, "Invalid graph parameters".into()))?;
    let index = state.search.current().ok_or_else(|| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Search index is not built".into(),
        )
    })?;
    let document_id = query.document.clone();
    let result =
        tokio::task::spawn_blocking(move || -> Result<RelatedView, crate::search::Error> {
            let document = index
                .document(&query.document)?
                .ok_or(crate::search::Error::InvalidQuery("Document not found"))?;
            Ok(RelatedView {
                topics: crate::search::related::topics(&document.title, &document.body),
                similar: crate::search::related::similar(
                    &index,
                    &query.document,
                    std::path::Path::new("data/similar.jsonl"),
                )?,
                generation: index.generation().to_string(),
            })
        })
        .await
        .map_err(|_| {
            Error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Related documents task failed".into(),
            )
        })?;
    let mut view = result.map_err(|error| {
        if matches!(error, crate::search::Error::InvalidQuery(_)) {
            Error(
                StatusCode::NOT_FOUND,
                "Document not found in search index".into(),
            )
        } else {
            tracing::error!(%error, "related documents failed");
            Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Could not read related documents".into(),
            )
        }
    })?;
    if let Some(candidates) = &mut view.similar {
        let futures = candidates.iter_mut().map(|candidate| async {
            if let Ok((a, b)) = state
                .semantic
                .chunk_texts(
                    &candidate.collection,
                    &document_id,
                    candidate.chunk_center,
                    &candidate.document_id,
                    candidate.chunk_other,
                )
                .await
            {
                candidate.text_center = a;
                candidate.text_other = b;
            }
        });
        futures_util::future::join_all(futures).await;
    }
    Ok(Json(view))
}

/// The whole reference graph: every document with at least one edge, and the edges between them.
#[utoipa::path(
    get,
    path = "/api/graph/overview",
    responses((status = 200, body = GraphOverview)),
)]
pub async fn overview(State(state): State<AppState>) -> Result<Json<GraphOverview>, Error> {
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let graph = graph::overview::execute(&mut tx).await?;
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
