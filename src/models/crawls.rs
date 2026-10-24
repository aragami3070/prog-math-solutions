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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Blocked,
    Interrupted,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Interrupted => "interrupted",
        }
    }
}

/// What a recorded response was requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    Robots,
    Document,
}

impl FetchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Robots => "robots",
            Self::Document => "document",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrawlRun {
    pub id: i64,
    pub source: String,
    pub status: RunStatus,
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
    pub kind: FetchKind,
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

/// A page waiting in the queue. `url` is its canonical identity; `fetch_url` is what is requested.
pub struct QueuedPage {
    pub id: i64,
    pub url: String,
    pub fetch_url: String,
}

/// A discovered page: canonical identity plus the URL to request (it may carry a topic slug).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueLink {
    pub url: String,
    pub fetch_url: String,
}

impl QueueLink {
    pub fn same(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            fetch_url: url.clone(),
            url,
        }
    }
}

pub struct CollectedPage {
    pub id: i64,
    pub fetch_id: i64,
    pub parsed: Option<ParsedPage>,
    pub links: Vec<QueueLink>,
}
