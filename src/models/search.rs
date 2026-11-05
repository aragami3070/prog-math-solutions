use crate::{
    models::sources::Source,
    search::{Hit, Rewrite},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Words, `"a quoted phrase"`, or an identifier such as `rfc:3058`, `RFC 3058`, `3058`.
    pub q: String,
    pub source: Option<Source>,
    /// Only documents whose every page was collected.
    #[serde(default)]
    pub complete_only: bool,
    /// Repair misspelled words against the corpus vocabulary (adds lower-weight branches).
    #[serde(default)]
    pub fuzzy: bool,
    /// Expand abbreviations and aliases from the curated synonym table.
    #[serde(default)]
    pub synonyms: bool,
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn first_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

/// What the numbers in a response refer to.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchScope {
    /// `lexical`: BM25 over title and body. `fuzzy` / `synonyms` say which rewriting modes ran.
    pub mode: &'static str,
    pub fuzzy: bool,
    pub synonyms: bool,
    pub synonyms_version: &'static str,
    pub analyzer_version: &'static str,
    /// The index generation that answered; changes when the index is rebuilt.
    pub generation: String,
    pub indexed_documents: u64,
    /// Hits beyond this depth are not reachable.
    pub max_depth: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResults {
    pub query: String,
    pub hits: Vec<Hit>,
    /// Every document matching the query and filters in the index, not only this page.
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
    pub scope: SearchScope,
    /// What the rewriting modes added: corrections and expansions. Never silent.
    pub rewrite: Rewrite,
    pub took_ms: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RebuildResult {
    pub generation: String,
    pub documents: u64,
    pub took_ms: u64,
}

/// A document as the index needs it.
pub struct IndexRow {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub body_text: String,
    pub completeness: String,
}
