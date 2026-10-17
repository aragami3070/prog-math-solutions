use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use super::crawls::CrawlRun;

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceStats {
    pub source: String,
    pub documents: i64,
    pub complete: i64,
    pub partial: i64,
    pub unknown: i64,
    pub text_chars: i64,
    pub last_collected_at: Option<DateTime<Utc>>,
    pub paused: bool,
    pub pause_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Statistics {
    pub total_documents: i64,
    pub document_requests: i64,
    pub http_errors: i64,
    pub sources: Vec<SourceStats>,
    pub recent_runs: Vec<CrawlRun>,
}
