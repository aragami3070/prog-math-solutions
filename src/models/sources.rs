use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Internals,
    Rfc,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internals => "internals",
            Self::Rfc => "rfc",
        }
    }

    pub fn origin(self) -> &'static str {
        match self {
            Self::Internals => "https://internals.rust-lang.org",
            Self::Rfc => "https://rust-lang.github.io",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub excerpt: String,
    pub text_chars: i64,
    pub completeness: String,
    pub completeness_reason: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentPage {
    pub items: Vec<DocumentSummary>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
    pub total_pages: i64,
}

// The parser returns data, never writes SQL or makes HTTP calls.
pub struct ParsedDocument {
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub body_text: String,
    pub completeness: &'static str,
    pub completeness_reason: String,
    pub metadata: serde_json::Value,
}
