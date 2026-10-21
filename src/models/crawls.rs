use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::sources::{ParsedPage, Source};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CrawlRequest {
    pub source: Source,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrawlRun {
    pub id: i64,
    pub source: String,
    pub status: String,
    pub requested: i32,
    pub succeeded: i32,
    pub failed: i32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

pub struct FetchRecord<'a> {
    pub run_id: i64,
    pub url: &'a str,
    pub kind: &'a str,
    pub status: Option<u16>,
    pub content_type: Option<&'a str>,
    pub body: Option<&'a str>,
    pub error: Option<&'a str>,
}

pub struct CrawlPlan {
    pub source: Source,
    pub user_agent: Arc<str>,
    pub delay: Duration,
}

pub struct QueuedPage {
    pub id: i64,
    pub url: String,
}

pub struct CollectedPage {
    pub id: i64,
    pub fetch_id: i64,
    pub parsed: Option<ParsedPage>,
    pub links: Vec<String>,
}
