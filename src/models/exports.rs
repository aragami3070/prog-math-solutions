use super::sources::{Completeness, Source};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    #[default]
    Jsonl,
    Csv,
    Zip,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
            Self::Zip => "zip",
        }
    }
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Jsonl => "application/x-ndjson",
            Self::Csv => "text/csv; charset=utf-8",
            Self::Zip => "application/zip",
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, IntoParams)]
#[serde(default, deny_unknown_fields)]
pub struct ExportQuery {
    pub format: ExportFormat,
    pub source: Option<Source>,
    pub complete_only: bool,
    /// Prefix formula-like CSV cells with an apostrophe; JSON remains unchanged.
    pub spreadsheet_safe: bool,
}

pub struct ExportRow {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub body_text: String,
    pub content_hash: String,
    pub completeness: Completeness,
    pub completeness_reason: String,
    pub parser_version: String,
    pub fetched_at: DateTime<Utc>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

pub struct ExportPost {
    pub document_id: i64,
    pub post_number: i32,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub text: String,
    pub code: Value,
}

pub struct ExportLink {
    pub document_id: i64,
    pub post_number: i32,
    pub target_url: String,
    pub anchor_text: Option<String>,
}

pub struct ExportPage {
    pub document_id: i64,
    pub page_no: i32,
    pub url: String,
    pub fetch_id: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ExportDocument {
    pub schema_version: u32,
    pub document_id: String,
    pub source: String,
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub language: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub authors: Vec<String>,
    pub source_category: Option<String>,
    pub source_tags: Vec<String>,
    pub sections: Vec<Value>,
    pub body_text: String,
    pub links: Vec<Value>,
    pub content_hash: String,
    pub completeness: Completeness,
    pub completeness_reason: String,
    pub fetch_ids: Vec<i64>,
    pub parser_version: String,
    pub license_provenance: Value,
    /// License source and the fetched pages (number, URL, fetch ID) behind the document.
    pub metadata: Value,
}

pub struct ExportArtifact {
    pub file: tokio::fs::File,
    pub size: u64,
    pub count: u64,
    pub sha256: String,
    pub snapshot_at: DateTime<Utc>,
}
