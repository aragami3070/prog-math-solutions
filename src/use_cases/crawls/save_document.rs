use super::errors::Error;
use crate::{
    models::sources::{ParsedDocument, Source},
    repository::{crawls, sources},
};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    source: Source,
    fetch_id: i64,
    mut doc: ParsedDocument,
) -> Result<(), Error> {
    if source == Source::Internals {
        let previous = sources::document_metadata(tx, source, &doc.native_id).await?;
        merge_pages(&mut doc, previous, run_id, fetch_id);
    }
    let hash = format!("{:x}", Sha256::digest(doc.body_text.as_bytes()));
    sources::upsert_document(tx, source, fetch_id, doc, &hash).await?;
    crawls::succeeded(tx, run_id).await?;
    Ok(())
}

// Keep pages from this run only, replacing a page on retry instead of appending it.
fn merge_pages(
    doc: &mut ParsedDocument,
    previous: Option<serde_json::Value>,
    run_id: i64,
    fetch_id: i64,
) {
    use serde_json::{Value, json};
    let mut pages = previous
        .filter(|v| v["run_id"].as_i64() == Some(run_id))
        .and_then(|v| v["pages"].as_object().cloned())
        .unwrap_or_default();
    let key = doc.metadata["page_url"]
        .as_str()
        .unwrap_or(&doc.canonical_url)
        .to_owned();
    pages.insert(key, json!({"fetch_id": fetch_id, "sections": doc.metadata["sections"], "links": doc.metadata["links"]}));
    let mut sections = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut links = std::collections::BTreeSet::new();
    for page in pages.values() {
        if let Some(parts) = page["sections"].as_array() {
            for part in parts {
                let key = part["post_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| part["text"].as_str().unwrap_or(""))
                    .to_owned();
                if seen.insert(key) {
                    sections.push(part.clone());
                }
            }
        }
        if let Some(urls) = page["links"].as_array() {
            links.extend(urls.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    sections.sort_by_key(|s| {
        s["post_id"]
            .as_str()
            .and_then(|id| id.strip_prefix("post_"))
            .and_then(|id| id.parse::<u64>().ok())
            .unwrap_or(u64::MAX)
    });
    doc.body_text = sections
        .iter()
        .filter_map(|s| s["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    doc.metadata["pages"] = json!(pages);
    doc.metadata["sections"] = json!(sections);
    doc.metadata["links"] = json!(links);
    doc.metadata["run_id"] = json!(run_id);
    doc.completeness = "unknown";
    doc.completeness_reason = "Collected HTML pages merged by post ID; full Discourse post count not independently verified".into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawler::sources::parse;

    #[test]
    fn merges_pages_without_duplicate_posts_and_replaces_retry() {
        let html = include_str!("../../../tests/fixtures/internals.html");
        let mut first = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/42",
            html,
        )
        .unwrap();
        merge_pages(&mut first, None, 5, 1);
        let body = first.body_text.clone();
        let mut second = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/42?page=2",
            html,
        )
        .unwrap();
        merge_pages(&mut second, Some(first.metadata), 5, 2);
        assert_eq!(second.body_text, body);
        assert_eq!(second.metadata["pages"].as_object().unwrap().len(), 2);
        let mut next_run = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/42",
            html,
        )
        .unwrap();
        merge_pages(&mut next_run, Some(second.metadata), 6, 3);
        assert_eq!(next_run.metadata["pages"].as_object().unwrap().len(), 1);
    }
}
