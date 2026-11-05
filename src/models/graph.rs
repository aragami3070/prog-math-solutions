use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 200;

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct GraphQuery {
    /// Document ID: `rfc:3058` or `internals:16678`.
    pub document: String,
    /// Most neighbour nodes returned (default 100, at most 200).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GraphNode {
    pub id: String,
    pub db_id: i64,
    pub source: String,
    pub title: String,
    pub url: String,
    pub completeness: String,
    /// `center`, `references` (the center links to it), `referenced_by` or `both`.
    pub role: &'static str,
}

/// One post that holds the link: where the edge comes from.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Evidence {
    pub post_id: i64,
    pub url: String,
    pub anchor_text: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GraphEdge {
    /// Node ID of the linking document.
    pub source: String,
    /// Node ID of the linked document.
    pub target: String,
    pub kind: &'static str,
    /// Links from `source` to `target`, all posts together.
    pub occurrences: i32,
    /// Up to ten of the links, with their URLs.
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GraphLimits {
    pub nodes: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Graph {
    pub center: GraphNode,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub edge_types: Vec<&'static str>,
    /// When the edges were last rebuilt from the stored links; `null` if never.
    pub built_at: Option<DateTime<Utc>>,
    /// Neighbours existed beyond the node limit and were left out (most-linked first kept).
    pub truncated: bool,
    /// Edges touching the center, including those left out.
    pub total_edges: i64,
    pub limits: GraphLimits,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GraphRebuild {
    pub edges: u64,
    pub took_ms: u64,
}

/// A document's identity and card fields.
pub struct NodeRow {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub title: String,
    pub canonical_url: String,
    pub completeness: String,
}

pub struct EdgeRow {
    pub source_id: i64,
    pub target_id: i64,
    pub occurrences: i32,
    pub evidence: serde_json::Value,
}
