use crate::{
    models::{events::ExperimentInfo, sources::Source},
    search::{Hit, Rewrite, semantic::Mode},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Words, `"a quoted phrase"`, or an identifier such as `rfc:3058`, `RFC 3058`, `3058`.
    #[serde(default)]
    pub q: String,
    /// Exact assigned v3 topic ID (lexical mode only).
    pub topic: Option<String>,
    pub source: Option<Source>,
    /// Only documents whose every page was collected.
    #[serde(default)]
    pub complete_only: bool,
    /// Only topics whose first post is from this year or later (1990..2100). Documents without
    /// a date (all RFCs) are left out whenever a year bound is given.
    pub year_from: Option<i32>,
    /// Only topics whose first post is from this year or earlier.
    pub year_to: Option<i32>,
    /// `lexical` (default), `vector` (embeddings in Qdrant) or `hybrid` (both, fused by RRF).
    /// If the vector side is down the answer is lexical and `scope.degraded` says why.
    #[serde(default)]
    pub mode: Mode,
    /// Vector and hybrid only: how many candidates each ranking contributes (10..500, default
    /// 100). Results cannot be paged deeper than this, and `total` is this pool, not a count of
    /// matching documents. Larger pools reach further down the ranking and cost more per query.
    pub pool: Option<u32>,
    /// Repair misspelled words against the corpus vocabulary (adds lower-weight branches).
    #[serde(default)]
    pub fuzzy: bool,
    /// Fuzzy matching inside the index (Tantivy `FuzzyTermQuery`): each query word also finds
    /// indexed words within an edit distance (1 for words up to 5 letters, else 2; words under 4
    /// letters and identifiers with digits stay exact). The query is not rewritten; exact hits
    /// rank above near ones.
    #[serde(default)]
    pub fuzzy_match: bool,
    /// Expand abbreviations and aliases from the curated synonym table.
    #[serde(default)]
    pub synonyms: bool,
    /// Also match other forms of a word (`objects` for `object`) via the lemma table.
    #[serde(default)]
    pub lemmas: bool,
    /// A random session id (8..64 characters of `A-Za-z0-9_-`) made by the client. With it the
    /// request joins the running experiment: the arm assigned to the session decides `mode`
    /// and the rewriting flags, whatever they say here, and the search is recorded.
    pub session: Option<String>,
    /// The `scope.generation` the earlier pages came from. If the index was rebuilt since, the
    /// answer is 409 instead of a page of a different ranking: start again from page 1.
    pub generation: Option<String>,
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
    /// The mode asked for; differs from `mode` when the answer degraded.
    pub requested_mode: &'static str,
    /// Why the answer is not what was asked for (vector store or embedding worker down).
    pub degraded: Option<String>,
    /// `total` counts every matching document (lexical) or only the candidate pool (vector,
    /// hybrid: at most 100).
    pub total_is_pool: bool,
    /// The candidate pool used by vector and hybrid modes.
    pub pool: u32,
    pub fuzzy: bool,
    pub fuzzy_match: bool,
    pub synonyms: bool,
    pub source: Option<String>,
    pub complete_only: bool,
    pub topic: Option<String>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub synonyms_version: &'static str,
    pub lemmas: bool,
    /// Version of the lemma table when one is loaded; `null` when the mode cannot run.
    pub lemmas_version: Option<String>,
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
    /// Present when the request carried a `session`.
    pub experiment: Option<ExperimentInfo>,
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
    pub year: Option<i32>,
}
