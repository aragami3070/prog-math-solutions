use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A click on a result of an earlier search.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClickEvent {
    /// The `experiment.query_id` the search response carried.
    pub query_id: String,
    /// `rfc:3058`, `internals:16678`.
    pub document_id: String,
    /// Position of the result in the full ranking, from 1.
    pub rank: u32,
}

/// Which arm answered a search; present only when the request carried a `session`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExperimentInfo {
    pub id: &'static str,
    pub version: u32,
    pub variant: &'static str,
    /// Send it back with each click. `null` if the event could not be recorded.
    pub query_id: Option<String>,
}

/// What the server records about one search.
pub struct NewSearch<'a> {
    pub session_id: &'a str,
    pub query: &'a str,
    pub mode: &'a str,
    pub requested_mode: &'a str,
    pub source: Option<&'a str>,
    pub complete_only: bool,
    pub results: i32,
    pub latency_ms: i32,
    pub degraded: bool,
    pub generation: &'a str,
}

/// One row of `search_events`, as the analysis reads it (also the JSONL export format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub at: DateTime<Utc>,
    /// `search` or `click`.
    pub kind: String,
    pub session_id: String,
    pub variant: String,
    pub query_id: String,
    pub query: Option<String>,
    pub results: Option<i32>,
    pub latency_ms: Option<i32>,
    pub degraded: Option<bool>,
    pub document_id: Option<String>,
    pub rank: Option<i32>,
}
