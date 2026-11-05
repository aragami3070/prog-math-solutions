pub mod crawls;
pub mod errors;
pub mod health;
pub mod search;
pub mod sources;
pub mod statistics;

use crate::{
    config::Config,
    crawler::Crawler,
    models::{
        crawls::{CrawlRequest, CrawlRun, RunStatus},
        exports::ExportFormat,
        search::{RebuildResult, SearchResults, SearchScope},
        sources::{Completeness, DocumentPage, DocumentSummary, Source},
        statistics::{SourceStats, Statistics},
    },
    search::{
        Hit, SearchHandle, SnippetPart,
        semantic::{Mode, Semantic},
    },
};
use axum::{Json, Router, extract::DefaultBodyLimit, http::StatusCode};
use errors::ErrorBody;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub crawler: Crawler,
    pub export_slots: Arc<Semaphore>,
    pub search: Arc<SearchHandle>,
    pub semantic: Semantic,
}

impl AppState {
    pub fn new(pool: PgPool, config: &Config) -> Self {
        Self {
            crawler: Crawler::new(
                pool.clone(),
                config.crawler_user_agent.clone(),
                config.crawl_delay,
                config.fresh_for,
            ),
            pool,
            export_slots: Arc::new(Semaphore::new(1)),
            search: Arc::new(SearchHandle::open(&config.search_dir)),
            semantic: Semantic::from_env(),
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health::check,
        sources::documents,
        sources::export,
        statistics::get_statistics,
        crawls::start,
        sources::resume,
        search::search,
        search::rebuild
    ),
    components(schemas(
        Source,
        ExportFormat,
        CrawlRequest,
        CrawlRun,
        RunStatus,
        Completeness,
        DocumentSummary,
        DocumentPage,
        SourceStats,
        Statistics,
        SearchResults,
        SearchScope,
        Mode,
        RebuildResult,
        Hit,
        SnippetPart,
        ErrorBody
    ))
)]
pub struct ApiDoc;

pub fn router(state: AppState, web_dist: &std::path::Path) -> Router {
    let api = Router::new()
        .merge(health::router())
        .merge(sources::router())
        .merge(search::router())
        .merge(crawls::router())
        .merge(statistics::router())
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "Unknown API route".into(),
                }),
            )
        });
    Router::new()
        .nest("/api", api)
        .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", ApiDoc::openapi()))
        .fallback_service(
            ServeDir::new(web_dist).fallback(ServeFile::new(web_dist.join("index.html"))),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests;
