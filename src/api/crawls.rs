use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    models::crawls::{CrawlRequest, CrawlRun},
    use_cases::crawls,
};
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    routing::post,
};
use sqlx::Connection;

pub fn router() -> Router<AppState> {
    Router::new().route("/crawls", post(start))
}

/// Crawl a whole source through HTML links, or resume its unfinished queue.
/// The response acknowledges a background job; progress is available in GET /api/stats.
#[utoipa::path(
    post,
    path = "/api/crawls",
    request_body = CrawlRequest,
    responses(
        (status = 202, body = CrawlRun),
        (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    body: Result<Json<CrawlRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CrawlRun>), Error> {
    let Json(request) = body.map_err(|_| {
        Error(
            StatusCode::BAD_REQUEST,
            "Expected a JSON crawl request of at most 64 KiB".into(),
        )
    })?;
    let plan = crawls::start::prepare(&state.crawler, request)?;
    let mut connection = state.pool.acquire().await?;
    // This session retains the crawl lock after commit and is never returned locked to the pool.
    connection.close_on_drop();
    let run = {
        let mut tx = connection.begin().await?;
        let run = crawls::start::execute(&mut tx, &plan).await?;
        tx.commit().await?;
        run
    };
    // No external requests may start until the run transaction is committed.
    state.crawler.spawn(run.id, plan, connection);
    Ok((StatusCode::ACCEPTED, Json(run)))
}
