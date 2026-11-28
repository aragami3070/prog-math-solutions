//! Experimental document-topic relations and unreviewed similarity candidates.
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Error, SearchIndex};

struct Rule {
    id: &'static str,
    label: &'static str,
    terms: &'static [&'static str],
}

// These conservative phrases are a transparent baseline, not model or human labels.
const RULES: &[Rule] = &[
    Rule {
        id: "ownership-borrowing",
        label: "Владение и заимствование",
        terms: &["ownership", "borrow checker", "borrowing"],
    },
    Rule {
        id: "lifetimes",
        label: "Времена жизни",
        terms: &["lifetime", "lifetimes"],
    },
    Rule {
        id: "traits-generics",
        label: "Трейты и обобщения",
        terms: &["trait", "traits", "generic", "generics"],
    },
    Rule {
        id: "async-concurrency",
        label: "Асинхронность и конкурентность",
        terms: &["async", "concurrency", "concurrent"],
    },
    Rule {
        id: "type-system",
        label: "Система типов",
        terms: &["type system", "type inference", "type checking"],
    },
    Rule {
        id: "error-handling",
        label: "Обработка ошибок",
        terms: &["error handling", "error type", "error propagation"],
    },
    Rule {
        id: "macros",
        label: "Макросы",
        terms: &["macro", "macros", "macro_rules"],
    },
    Rule {
        id: "unsafe-memory",
        label: "Unsafe и память",
        terms: &["unsafe", "memory safety", "undefined behavior"],
    },
    Rule {
        id: "compiler",
        label: "Компилятор",
        terms: &["compiler", "rustc"],
    },
    Rule {
        id: "standard-library",
        label: "Стандартная библиотека",
        terms: &["standard library", "stdlib"],
    },
    Rule {
        id: "cargo-tooling",
        label: "Cargo и инструменты",
        terms: &["cargo", "rustfmt", "clippy"],
    },
    Rule {
        id: "performance",
        label: "Производительность",
        terms: &["performance", "benchmark", "optimization"],
    },
    Rule {
        id: "syntax-ergonomics",
        label: "Синтаксис и эргономика",
        terms: &["syntax", "ergonomics"],
    },
    Rule {
        id: "language-process",
        label: "Развитие языка",
        terms: &["language design", "language team", "rfc process"],
    },
];

#[derive(Debug, Serialize, ToSchema)]
pub struct TopicCandidate {
    pub id: String,
    pub label: String,
    pub matched_terms: Vec<String>,
    pub provenance: String,
    pub review_state: String,
}

/// Labels are taken from the same versioned taxonomy used by the classifier.
pub fn classification_topic(id: &str, review_state: &str) -> TopicCandidate {
    let taxonomy: serde_json::Value =
        serde_json::from_str(include_str!("../../tools/nlp/topics-v3.json"))
            .expect("embedded v3 taxonomy must be valid");
    let label = taxonomy["topics"]
        .as_array()
        .and_then(|topics| {
            topics
                .iter()
                .find(|topic| topic["id"].as_str() == Some(id))
                .and_then(|topic| topic["label"].as_str())
        })
        .unwrap_or(id);
    TopicCandidate {
        id: id.to_owned(),
        label: label.to_owned(),
        matched_terms: Vec::new(),
        provenance: "classification-v3".into(),
        review_state: review_state.to_owned(),
    }
}

fn contains_term(text: &str, term: &str) -> bool {
    text.match_indices(term).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + term.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

pub fn topics(title: &str, body: &str) -> Vec<TopicCandidate> {
    let text = format!("{}\n{}", title.to_lowercase(), body.to_lowercase());
    RULES
        .iter()
        .filter_map(|rule| {
            let matched_terms: Vec<_> = rule
                .terms
                .iter()
                .copied()
                .filter(|term| contains_term(&text, term))
                .map(str::to_owned)
                .collect();
            (!matched_terms.is_empty()).then_some(TopicCandidate {
                id: rule.id.to_owned(),
                label: rule.label.to_owned(),
                matched_terms,
                provenance: "keyword-baseline-v1".into(),
                review_state: "unreviewed".into(),
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct Pair {
    a: String,
    b: String,
    score: f64,
    chunk_a: usize,
    chunk_b: usize,
    chunk_score: f64,
    model: String,
    method: String,
    #[serde(default = "default_collection")]
    collection: String,
}

fn default_collection() -> String {
    "rks_chunks_all_v2".to_owned()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SimilarCandidate {
    pub document_id: String,
    pub title: String,
    pub url: String,
    pub score: f64,
    pub chunk_center: usize,
    pub chunk_other: usize,
    pub chunk_score: f64,
    pub model: String,
    pub method: String,
    pub collection: String,
    pub text_center: Option<String>,
    pub text_other: Option<String>,
}

/// Read an offline snapshot. Missing snapshot is distinct from an empty candidate list.
pub fn similar(
    index: &SearchIndex,
    document: &str,
    path: &std::path::Path,
) -> Result<Option<Vec<SimilarCandidate>>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut pairs = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let pair: Pair = serde_json::from_str(&line)?;
        if pair.a == document || pair.b == document {
            pairs.push(pair);
        }
    }
    pairs.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut result = Vec::new();
    for pair in pairs {
        let (other, chunk_center, chunk_other) = if pair.a == document {
            (&pair.b, pair.chunk_a, pair.chunk_b)
        } else {
            (&pair.a, pair.chunk_b, pair.chunk_a)
        };
        if let Some(doc) = index.document(other)? {
            result.push(SimilarCandidate {
                document_id: doc.document_id,
                title: doc.title,
                url: doc.url,
                score: pair.score,
                chunk_center,
                chunk_other,
                chunk_score: pair.chunk_score,
                model: pair.model,
                method: pair.method,
                collection: pair.collection,
                text_center: None,
                text_other: None,
            });
        }
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_are_boundary_aware_and_explain_matches() {
        let found = topics("Async traits", "The compiler checks borrowing.");
        assert!(
            found
                .iter()
                .any(|t| t.id == "async-concurrency" && t.matched_terms == ["async"])
        );
        assert!(found.iter().any(|t| t.id == "traits-generics"));
        assert!(found.iter().any(|t| t.id == "compiler"));
        assert!(topics("asynchronous", "traitor").is_empty());
    }
}
