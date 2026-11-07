use super::{
    Error, SearchIndex,
    rewrite::{Options, REWRITE_BOOST, Rewrite},
};
use serde::Serialize;
use std::ops::Bound;
use tantivy::{
    TantivyDocument, Term,
    collector::{Count, TopDocs},
    query::{
        BooleanQuery, BoostQuery, ConstScoreQuery, FuzzyTermQuery, Occur, PhraseQuery, Query,
        RangeQuery, TermQuery,
    },
    schema::{Field, IndexRecordOption, Value},
    snippet::SnippetGenerator,
};
use utoipa::ToSchema;

const MAX_QUERY_CHARS: usize = 200;
/// Hits deeper than this are not reachable: deep pages cost more and mean little for ranking.
pub const MAX_DEPTH: usize = 1_000;
const TITLE_BOOST: f32 = 3.0;
const TITLE_PHRASE_BOOST: f32 = 4.0;
const BODY_PHRASE_BOOST: f32 = 2.0;
const EXACT_ID_BOOST: f32 = 100.0;
/// Fuzzy branches weigh less than exact ones, so an exact hit still ranks above a near miss.
const FUZZY_MATCH_BOOST: f32 = 0.4;
/// Shorter words are matched exactly only: at distance 1 they would match half the vocabulary.
const FUZZY_MIN_LEN: usize = 4;

/// Lucene's `AUTO` edit distance: 1 for words up to five letters, 2 beyond.
fn fuzzy_distance(token: &str) -> Option<u8> {
    let len = token.chars().count();
    // Identifiers with digits (`e0308`, `3058`) are matched exactly: a near number is another one.
    if len < FUZZY_MIN_LEN || token.chars().any(|c| c.is_ascii_digit()) {
        None
    } else if len <= 5 {
        Some(1)
    } else {
        Some(2)
    }
}
const SNIPPET_CHARS: usize = 240;

#[derive(Debug, Default, Clone)]
pub struct Filters {
    pub source: Option<String>,
    pub complete_only: bool,
    /// Year of the first post, inclusive bounds. Documents without a date never match.
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}

/// A piece of the snippet; `matched` parts are what the query found. Plain text, never HTML.
#[derive(Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct SnippetPart {
    pub text: String,
    pub matched: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Hit {
    pub id: i64,
    pub document_id: String,
    pub source: String,
    pub title: String,
    pub url: String,
    pub completeness: String,
    pub score: f32,
    pub snippet: Vec<SnippetPart>,
}

pub struct Results {
    pub hits: Vec<Hit>,
    /// Every document matching the query and filters, not just the returned page.
    pub total: usize,
    /// What the rewriting modes added to the query; empty when none applied.
    pub rewrite: Rewrite,
}

impl SearchIndex {
    pub fn search(
        &self,
        text: &str,
        filters: &Filters,
        options: Options,
        offset: usize,
        limit: usize,
    ) -> Result<Results, Error> {
        if offset + limit > MAX_DEPTH {
            return Err(Error::InvalidQuery("results are limited to the first 1000"));
        }
        let (query, rewrite) = self.build_query(text, filters, options)?;
        let searcher = self.reader.searcher();
        let (total, top) = searcher.search(
            &query,
            &(
                Count,
                TopDocs::with_limit(limit)
                    .and_offset(offset)
                    .order_by_score(),
            ),
        )?;
        let f = self.fields;
        let snippets =
            SnippetGenerator::create(&searcher, &*query, f.body).map(|mut generator| {
                generator.set_max_num_chars(SNIPPET_CHARS);
                generator
            })?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher.doc(address)?;
            let body = self.stored_text(&doc, f.body);
            let snippet = snippets.snippet(&body);
            hits.push(Hit {
                id: doc
                    .get_first(f.db_id)
                    .and_then(|value| value.as_i64())
                    .unwrap_or_default(),
                document_id: self.stored_text(&doc, f.doc_id),
                source: self.stored_text(&doc, f.source),
                title: self.stored_text(&doc, f.title),
                url: self.stored_text(&doc, f.url),
                completeness: self.stored_text(&doc, f.completeness),
                score,
                snippet: parts(&snippet, &body),
            });
        }
        Ok(Results {
            hits,
            total,
            rewrite,
        })
    }

    fn build_query(
        &self,
        text: &str,
        filters: &Filters,
        options: Options,
    ) -> Result<(Box<dyn Query>, Rewrite), Error> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::InvalidQuery("query must not be empty"));
        }
        if text.chars().count() > MAX_QUERY_CHARS {
            return Err(Error::InvalidQuery("query is limited to 200 characters"));
        }
        let f = self.fields;
        let mut should: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // Exact identifiers outrank everything: `rfc:3058`, `RFC 3058`, a bare `3058`.
        // Several candidates (a bare number) rank in the order listed: the RFC before the topic.
        for (position, id) in exact_ids(text).into_iter().enumerate() {
            let term = Term::from_field_text(f.doc_id, &id);
            should.push((
                Occur::Should,
                boosted(
                    TermQuery::new(term, IndexRecordOption::Basic),
                    EXACT_ID_BOOST / (position + 1) as f32,
                ),
            ));
        }

        let (phrase_only, inner) = match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            Some(inner) if !inner.trim().is_empty() => (true, inner),
            _ => (false, text),
        };
        let terms = self.tokens(inner, f.body)?;
        // A quoted phrase is literal: rewriting it would change what the user asked for.
        let rewrite = if phrase_only {
            Rewrite::default()
        } else {
            self.rewrite(&terms, options, super::rewrite::Lemmas::shared())?
        };
        if !phrase_only {
            for token in &terms {
                for (field, boost) in [(f.title, TITLE_BOOST), (f.body, 1.0)] {
                    let term = Term::from_field_text(field, token);
                    should.push((
                        Occur::Should,
                        boosted(TermQuery::new(term, IndexRecordOption::WithFreqs), boost),
                    ));
                }
            }
        }
        if options.fuzzy_match && !phrase_only {
            for token in &terms {
                let Some(distance) = fuzzy_distance(token) else {
                    continue;
                };
                for (field, boost) in [(f.title, TITLE_BOOST), (f.body, 1.0)] {
                    let term = Term::from_field_text(field, token);
                    should.push((
                        Occur::Should,
                        boosted(
                            FuzzyTermQuery::new(term, distance, true),
                            boost * FUZZY_MATCH_BOOST,
                        ),
                    ));
                }
            }
        }
        // Phrase matching uses positions: "async fn" is not just two words somewhere.
        if terms.len() >= 2 {
            for (field, boost) in [(f.title, TITLE_PHRASE_BOOST), (f.body, BODY_PHRASE_BOOST)] {
                let phrase = PhraseQuery::new(
                    terms
                        .iter()
                        .map(|t| Term::from_field_text(field, t))
                        .collect(),
                );
                should.push((Occur::Should, boosted(phrase, boost)));
            }
        } else if phrase_only {
            // A quoted single token is a plain term search.
            for field in [f.title, f.body] {
                let term = Term::from_field_text(field, &terms[0]);
                should.push((
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs)),
                ));
            }
        }
        for token in &rewrite.terms {
            for (field, boost) in [(f.title, TITLE_BOOST), (f.body, 1.0)] {
                let term = Term::from_field_text(field, token);
                should.push((
                    Occur::Should,
                    boosted(
                        TermQuery::new(term, IndexRecordOption::WithFreqs),
                        boost * REWRITE_BOOST,
                    ),
                ));
            }
        }
        for tokens in &rewrite.phrases {
            for (field, boost) in [(f.title, TITLE_PHRASE_BOOST), (f.body, BODY_PHRASE_BOOST)] {
                let query: Box<dyn Query> = if tokens.len() >= 2 {
                    Box::new(PhraseQuery::new(
                        tokens
                            .iter()
                            .map(|t| Term::from_field_text(field, t))
                            .collect(),
                    ))
                } else {
                    Box::new(TermQuery::new(
                        Term::from_field_text(field, &tokens[0]),
                        IndexRecordOption::WithFreqs,
                    ))
                };
                should.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(query, boost * REWRITE_BOOST)),
                ));
            }
        }
        let mut must: Vec<(Occur, Box<dyn Query>)> =
            vec![(Occur::Must, Box::new(BooleanQuery::new(should)))];
        if let Some(source) = &filters.source {
            must.push((Occur::Must, filter(f.source, source)));
        }
        if filters.complete_only {
            must.push((Occur::Must, filter(f.completeness, "complete")));
        }
        if filters.year_from.is_some() || filters.year_to.is_some() {
            let bound = |year: Option<i32>| {
                year.map_or(Bound::Unbounded, |y| {
                    Bound::Included(Term::from_field_i64(f.year, i64::from(y)))
                })
            };
            must.push((
                Occur::Must,
                Box::new(RangeQuery::new(
                    bound(filters.year_from),
                    bound(filters.year_to),
                )),
            ));
        }
        Ok((Box::new(BooleanQuery::new(must)), rewrite))
    }

    /// The query text through the same analyzer the documents went through.
    pub(super) fn tokens(&self, text: &str, field: Field) -> Result<Vec<String>, Error> {
        let mut analyzer = self.index.tokenizer_for_field(field)?;
        let mut stream = analyzer.token_stream(text);
        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        if tokens.is_empty() {
            return Err(Error::InvalidQuery("query has no searchable terms"));
        }
        Ok(tokens)
    }
}

fn boosted(query: impl Query + 'static, boost: f32) -> Box<dyn Query> {
    Box::new(BoostQuery::new(Box::new(query), boost))
}

/// A restriction that narrows the matches without changing their scores.
fn filter(field: Field, value: &str) -> Box<dyn Query> {
    let term = Term::from_field_text(field, value);
    Box::new(ConstScoreQuery::new(
        Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        0.0,
    ))
}

/// Document IDs a query may be naming outright.
pub(super) fn exact_ids(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    if let Some((source, id)) = lower.split_once(':')
        && matches!(source, "rfc" | "internals")
        && let Some(id) = number(id)
    {
        return vec![format!("{source}:{id}")];
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        [word] if word.starts_with("rfc") && number(word.trim_start_matches("rfc")).is_some() => {
            vec![format!(
                "rfc:{}",
                number(word.trim_start_matches("rfc")).unwrap()
            )]
        }
        ["rfc", id] | ["rfc", "#", id] => number(id)
            .map(|id| format!("rfc:{id}"))
            .into_iter()
            .collect(),
        [id] => number(id)
            .map(|id| vec![format!("rfc:{id}"), format!("internals:{id}")])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `3058`, `0001` or `#3058` as the stored form (no leading zeros).
fn number(text: &str) -> Option<String> {
    let digits = text.trim().trim_start_matches('#');
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: u64 = digits.parse().ok()?;
    (value > 0).then(|| value.to_string())
}

/// Splits the snippet fragment into matched and plain parts. The fragment is a slice of
/// `body`, so highlight ranges are byte offsets into the fragment.
fn parts(snippet: &tantivy::snippet::Snippet, body: &str) -> Vec<SnippetPart> {
    let fragment = snippet.fragment();
    if fragment.is_empty() {
        // Matched by title or ID only: show the start of the text instead.
        let head: String = body.chars().take(SNIPPET_CHARS).collect();
        return if head.is_empty() {
            Vec::new()
        } else {
            vec![SnippetPart {
                text: head,
                matched: false,
            }]
        };
    }
    let mut parts = Vec::new();
    let mut at = 0;
    for range in snippet.highlighted() {
        if range.start > at {
            parts.push(SnippetPart {
                text: fragment[at..range.start].to_owned(),
                matched: false,
            });
        }
        parts.push(SnippetPart {
            text: fragment[range.clone()].to_owned(),
            matched: true,
        });
        at = range.end;
    }
    if at < fragment.len() {
        parts.push(SnippetPart {
            text: fragment[at..].to_owned(),
            matched: false,
        });
    }
    parts
}

/// A document as stored in the index, for tools that need its text (evaluation pools).
pub struct StoredDocument {
    pub document_id: String,
    pub source: String,
    pub title: String,
    pub url: String,
    pub completeness: String,
    pub body: String,
}

impl SearchIndex {
    pub fn document(&self, document_id: &str) -> Result<Option<StoredDocument>, Error> {
        let f = self.fields;
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(f.doc_id, document_id);
        let found = searcher.search(
            &TermQuery::new(term, IndexRecordOption::Basic),
            &TopDocs::with_limit(1).order_by_score(),
        )?;
        let Some((_, address)) = found.into_iter().next() else {
            return Ok(None);
        };
        let doc: TantivyDocument = searcher.doc(address)?;
        Ok(Some(StoredDocument {
            document_id: document_id.to_owned(),
            source: self.stored_text(&doc, f.source),
            title: self.stored_text(&doc, f.title),
            url: self.stored_text(&doc, f.url),
            completeness: self.stored_text(&doc, f.completeness),
            body: self.stored_text(&doc, f.body),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_recognised_in_their_usual_spellings() {
        assert_eq!(exact_ids("rfc:3058"), ["rfc:3058"]);
        assert_eq!(exact_ids("RFC 3058"), ["rfc:3058"]);
        assert_eq!(exact_ids("rfc3058"), ["rfc:3058"]);
        assert_eq!(exact_ids("RFC 0001"), ["rfc:1"]);
        assert_eq!(exact_ids("3058"), ["rfc:3058", "internals:3058"]);
        assert_eq!(exact_ids("internals:42"), ["internals:42"]);
        assert!(exact_ids("async fn").is_empty());
        assert!(exact_ids("rfc 3058 async").is_empty());
        assert!(exact_ids("rfc:0").is_empty());
    }
}
