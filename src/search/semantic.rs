//! Semantic and hybrid retrieval: chunk vectors in Qdrant, query vectors from the local
//! embedding worker (`tools/nlp/worker.py`), fused with the lexical ranking by RRF.
//!
//! Both services are optional. When either is down the answer is the plain lexical one and
//! says so (`degraded`): a search page that fails because a model server is off is worse
//! than one that says "vectors unavailable".

use super::{Error, Filters, Hit, Options, Rewrite, SearchIndex, SnippetPart};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use utoipa::ToSchema;

const COLLECTION: &str = "rks_chunks_v2";
/// Candidates taken from each ranking before fusion when the request does not say; hybrid pages
/// are cut from this pool. The client may ask for `MIN_POOL..=MAX_POOL`.
pub const POOL: usize = 100;
pub const MIN_POOL: usize = 10;
pub const MAX_POOL: usize = 500;
/// Chunks fetched per wanted document: several chunks of one document collapse into one.
const CHUNKS_PER_DOCUMENT: usize = 3;
/// Standard RRF damping constant.
const RRF_K: f32 = 60.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Lexical,
    Vector,
    Hybrid,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Vector => "vector",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone)]
pub struct Semantic {
    http: reqwest::Client,
    qdrant: String,
    embed: String,
    collection: String,
}

/// The best chunk of a document and how close it is to the query.
pub struct DocumentMatch {
    pub db_id: i64,
    pub document_id: String,
    pub score: f32,
    pub chunk: String,
}

#[derive(Deserialize)]
struct Embedded {
    vectors: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct Found {
    result: Vec<Point>,
}

#[derive(Deserialize)]
struct Point {
    score: f32,
    payload: Payload,
}

#[derive(Deserialize)]
struct Payload {
    db_id: i64,
    doc_id: String,
    text: String,
}

impl Semantic {
    pub fn new(qdrant: &str, embed: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("a plain HTTP client builds"),
            qdrant: qdrant.trim_end_matches('/').to_owned(),
            embed: embed.trim_end_matches('/').to_owned(),
            collection: COLLECTION.to_owned(),
        }
    }

    /// `QDRANT_URL` (default `http://127.0.0.1:6333`) and `EMBED_URL` (default
    /// `http://127.0.0.1:8090`), both loopback services.
    pub fn from_env() -> Self {
        let url = |name: &str, default: &str| {
            std::env::var(name)
                .unwrap_or_else(|_| default.to_owned())
                .trim_end_matches('/')
                .to_owned()
        };
        let mut semantic = Self::new(
            &url("QDRANT_URL", "http://127.0.0.1:6333"),
            &url("EMBED_URL", "http://127.0.0.1:8090"),
        );
        semantic.collection = url("QDRANT_COLLECTION", COLLECTION);
        semantic
    }

    /// Fetch stored previews for a pair from the collection that produced the snapshot.
    pub async fn chunk_texts(
        &self,
        collection: &str,
        a: &str,
        a_ordinal: usize,
        b: &str,
        b_ordinal: usize,
    ) -> Result<(Option<String>, Option<String>), String> {
        if collection.is_empty()
            || !collection
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("invalid collection name".into());
        }
        let mut texts = Vec::new();
        for (id, ordinal) in [(a, a_ordinal), (b, b_ordinal)] {
            let response = self.http.post(format!("{}/collections/{}/points/scroll", self.qdrant, collection))
                .json(&serde_json::json!({"limit": 1, "with_payload": ["text"], "with_vector": false,
                    "filter": {"must": [{"key": "doc_id", "match": {"value": id}}, {"key": "ordinal", "match": {"value": ordinal}}]}}))
                .send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
            let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            texts.push(
                value["result"]["points"]
                    .as_array()
                    .and_then(|p| p.first())
                    .and_then(|p| p["payload"]["text"].as_str())
                    .map(str::to_owned),
            );
        }
        Ok((texts.remove(0), texts.remove(0)))
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, String> {
        let response = self
            .http
            .post(format!("{}/embed", self.embed))
            .json(&serde_json::json!({"texts": [text], "kind": "query"}))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("embedding worker unavailable ({e})"))?;
        let embedded: Embedded = response
            .json()
            .await
            .map_err(|e| format!("embedding worker answered badly ({e})"))?;
        embedded
            .vectors
            .into_iter()
            .next()
            .ok_or_else(|| "embedding worker returned no vector".to_owned())
    }

    /// Nearest documents to the query, one entry per document (its best chunk). Filters are the
    /// ones the lexical side uses, applied inside Qdrant.
    pub async fn nearest(
        &self,
        text: &str,
        filters: &Filters,
        pool: usize,
    ) -> Result<Vec<DocumentMatch>, String> {
        let vector = self.embed_query(text).await?;
        let mut must = Vec::new();
        if let Some(source) = &filters.source {
            must.push(serde_json::json!({"key": "source", "match": {"value": source}}));
        }
        if filters.complete_only {
            must.push(serde_json::json!({"key": "completeness", "match": {"value": "complete"}}));
        }
        if filters.year_from.is_some() || filters.year_to.is_some() {
            let mut range = serde_json::Map::new();
            if let Some(from) = filters.year_from {
                range.insert("gte".into(), from.into());
            }
            if let Some(to) = filters.year_to {
                range.insert("lte".into(), to.into());
            }
            must.push(serde_json::json!({"key": "year", "range": range}));
        }
        let response = self
            .http
            .post(format!(
                "{}/collections/{}/points/search",
                self.qdrant, self.collection
            ))
            .json(&serde_json::json!({
                "vector": vector,
                "limit": pool * CHUNKS_PER_DOCUMENT,
                "with_payload": true,
                "filter": {"must": must},
            }))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("vector store unavailable ({e})"))?;
        let found: Found = response
            .json()
            .await
            .map_err(|e| format!("vector store answered badly ({e})"))?;
        // Points come best first, so the first chunk seen for a document is its best.
        let mut seen = HashMap::new();
        let mut documents = Vec::new();
        for point in found.result {
            if seen.insert(point.payload.doc_id.clone(), ()).is_none() {
                documents.push(DocumentMatch {
                    db_id: point.payload.db_id,
                    document_id: point.payload.doc_id,
                    score: point.score,
                    chunk: point.payload.text,
                });
                if documents.len() == pool {
                    break;
                }
            }
        }
        Ok(documents)
    }
}

pub struct Answer {
    pub hits: Vec<Hit>,
    pub total: usize,
    /// `total` counts the candidate pool (vector and hybrid), not every matching document.
    pub total_is_pool: bool,
    pub rewrite: Rewrite,
    /// The mode that actually produced the hits.
    pub mode: Mode,
    pub degraded: Option<String>,
}

/// Reciprocal rank fusion: documents ranked well by either list rise, those ranked by both
/// rise most. Scores of the two rankings are not comparable, so only ranks are used.
fn fuse(lists: &[Vec<String>]) -> Vec<(String, f32)> {
    let mut score: HashMap<&str, f32> = HashMap::new();
    for list in lists {
        for (rank, id) in list.iter().enumerate() {
            *score.entry(id).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(String, f32)> = score.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    // Ties (equal fused score) fall back to the id so the order is deterministic.
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    fused
}

/// One search: what to look for, how, and which slice of the ranking to return.
pub struct Request {
    pub text: String,
    pub filters: Filters,
    pub options: Options,
    pub mode: Mode,
    /// Candidates taken from each ranking (vector and hybrid); `MIN_POOL..=MAX_POOL`.
    pub pool: usize,
    pub offset: usize,
    pub limit: usize,
}

pub async fn answer(
    semantic: &Semantic,
    index: Arc<SearchIndex>,
    request: Request,
) -> Result<Answer, Error> {
    let Request {
        text,
        filters,
        options,
        mode,
        pool,
        offset,
        limit,
    } = request;
    let lexical = |index: Arc<SearchIndex>, text: String, filters: Filters, offset, limit| async move {
        tokio::task::spawn_blocking(move || index.search(&text, &filters, options, offset, limit))
            .await?
    };
    if mode == Mode::Lexical {
        let found = lexical(index, text, filters, offset, limit).await?;
        return Ok(Answer {
            hits: found.hits,
            total: found.total,
            total_is_pool: false,
            rewrite: found.rewrite,
            mode,
            degraded: None,
        });
    }
    if offset + limit > pool {
        return Err(Error::InvalidQuery(
            "vector and hybrid results are limited to the candidate pool (parameter pool)",
        ));
    }
    let nearest = match semantic.nearest(&text, &filters, pool).await {
        Ok(nearest) => nearest,
        Err(reason) => {
            // Degrade, never fail: the lexical ranking still answers.
            let found = lexical(index, text, filters, offset, limit).await?;
            return Ok(Answer {
                hits: found.hits,
                total: found.total,
                total_is_pool: false,
                rewrite: found.rewrite,
                mode: Mode::Lexical,
                degraded: Some(reason),
            });
        }
    };
    let chunks: HashMap<String, (i64, String)> = nearest
        .iter()
        .map(|m| (m.document_id.clone(), (m.db_id, m.chunk.clone())))
        .collect();
    let vector_order: Vec<String> = nearest.iter().map(|m| m.document_id.clone()).collect();

    let (mut lexical_hits, rewrite, order) = if mode == Mode::Hybrid {
        let found = lexical(index.clone(), text.clone(), filters, 0, pool).await;
        // A query the lexical side rejects (no searchable terms) still has a vector answer.
        let (hits, rewrite) = match found {
            Ok(found) => (found.hits, found.rewrite),
            Err(Error::InvalidQuery(_)) => (Vec::new(), Rewrite::default()),
            Err(error) => return Err(error),
        };
        let lexical_order: Vec<String> = hits.iter().map(|h| h.document_id.clone()).collect();
        let mut fused = fuse(&[lexical_order, vector_order]);
        // A document the query names outright (`RFC 3058`) stays first: embeddings know
        // nothing about numbers, and rank fusion would bury it.
        let named: Vec<String> = super::query::exact_ids(&text)
            .into_iter()
            .filter(|id| hits.iter().any(|h| &h.document_id == id))
            .collect();
        fused.retain(|(id, _)| !named.contains(id));
        let pinned = named.into_iter().map(|id| (id, 1.0));
        fused = pinned.chain(fused).collect();
        (hits, rewrite, fused)
    } else {
        let order = vector_order
            .iter()
            .zip(&nearest)
            .map(|(id, m)| (id.clone(), m.score))
            .collect();
        (Vec::new(), Rewrite::default(), order)
    };

    let total = order.len();
    let page: Vec<(String, f32)> = order.into_iter().skip(offset).take(limit).collect();
    let mut by_id: HashMap<String, Hit> = lexical_hits
        .drain(..)
        .map(|hit| (hit.document_id.clone(), hit))
        .collect();
    let mut hits = Vec::with_capacity(page.len());
    for (document_id, score) in page {
        let mut hit = match by_id.remove(&document_id) {
            Some(hit) => hit,
            None => {
                let (db_id, chunk) = chunks.get(&document_id).cloned().unwrap_or_default();
                let index = index.clone();
                let id = document_id.clone();
                // The vector store only knows the chunk; the rest comes from the lexical index.
                let stored = tokio::task::spawn_blocking(move || index.document(&id)).await??;
                let Some(stored) = stored else {
                    continue; // deleted since the vectors were built
                };
                Hit {
                    id: db_id,
                    document_id: stored.document_id,
                    source: stored.source,
                    title: stored.title,
                    url: stored.url,
                    completeness: stored.completeness,
                    score,
                    snippet: vec![SnippetPart {
                        text: chunk,
                        matched: false,
                    }],
                }
            }
        };
        hit.score = score;
        hits.push(hit);
    }
    Ok(Answer {
        hits,
        total,
        total_is_pool: true,
        rewrite,
        mode,
        degraded: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[(String, f32)]) -> Vec<&str> {
        items.iter().map(|(id, _)| id.as_str()).collect()
    }

    #[test]
    fn a_document_found_by_both_rankings_rises_above_one_found_by_one() {
        let lexical = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let vector = vec!["c".to_owned(), "d".to_owned(), "b".to_owned()];
        let fused = fuse(&[lexical, vector]);
        // b and c are in both lists; c is first in one, third in the other, b second and third.
        assert_eq!(ids(&fused), ["c", "b", "a", "d"]);
    }

    #[test]
    fn fusion_order_is_deterministic_on_ties() {
        let fused = fuse(&[vec!["x".to_owned()], vec!["y".to_owned()]]);
        assert_eq!(ids(&fused), ["x", "y"]);
    }
}
