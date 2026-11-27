use crate::models::search::IndexRow;
use futures_util::stream::BoxStream;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Every document, streamed so the corpus is never held in memory at once.
pub fn documents<'a>(
    tx: &'a mut Transaction<'_, Postgres>,
) -> BoxStream<'a, Result<IndexRow, sqlx::Error>> {
    sqlx::query_as!(
        IndexRow,
        r#"
        SELECT
            id, source, native_id, canonical_url, title, body_text,
            completeness AS "completeness!",
            (SELECT EXTRACT(YEAR FROM p.published_at AT TIME ZONE 'UTC')::INT
             FROM posts AS p WHERE p.document_id = documents.id AND p.post_number = 1) AS year
        FROM documents
        ORDER BY id
        "#,
    )
    .fetch(&mut **tx)
}

/// What the index must reflect: changes when a document appears, goes, or changes content or
/// completeness. Not affected by a recrawl that found the same content.
pub async fn corpus_marker(tx: &mut Transaction<'_, Postgres>) -> Result<String, sqlx::Error> {
    let marker = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(md5(string_agg(
            source || ':' || native_id || ':' || content_hash || ':' || completeness || ':' || title,
            ',' ORDER BY id)), '') || ':' || COUNT(*)::TEXT AS "marker!"
        FROM documents
        "#
    )
    .fetch_one(&mut **tx)
    .await?;
    let classification: String = sqlx::query_scalar("SELECT md5(COALESCE((SELECT string_agg(row_to_json(r)::text, ',' ORDER BY r.id) FROM classification_runs r), '') || COALESCE((SELECT string_agg(row_to_json(t)::text, ',' ORDER BY t.run_id, t.topic_id) FROM classification_topics t), '') || COALESCE((SELECT string_agg(row_to_json(e)::text, ',' ORDER BY e.id) FROM classification_review_events e), '') || COALESCE((SELECT string_agg(row_to_json(p)::text, ',' ORDER BY p.document_id, p.post_number) FROM posts p), ''))").fetch_one(&mut **tx).await?;
    Ok(format!("{marker}:{classification}"))
}

/// Resolve v3 assignments against the complete current classifier input in this snapshot.
pub async fn effective_topics(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<HashMap<i64, Vec<String>>, sqlx::Error> {
    let rows = sqlx::query("SELECT d.id, d.title, p.post_number, p.text FROM documents d JOIN posts p ON p.document_id=d.id ORDER BY d.id, p.post_number").fetch_all(&mut **tx).await?;
    let mut inputs: BTreeMap<i64, (String, BTreeMap<String, String>)> = BTreeMap::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let n: i32 = row.try_get("post_number")?;
        let text: String = row.try_get("text")?;
        inputs
            .entry(id)
            .or_insert_with(|| (title, BTreeMap::new()))
            .1
            .insert(format!("post-{n}"), text);
    }
    let hashes: HashMap<i64, String> = inputs
        .into_iter()
        .map(|(id, (title, sections))| {
            let data = serde_json::json!({"sections": sections, "title": title});
            let digest = Sha256::digest(serde_json::to_vec(&data).expect("strings serialize"));
            (id, format!("{digest:x}"))
        })
        .collect();
    let runs = sqlx::query("SELECT id, document_id, content_hash, status FROM classification_runs WHERE taxonomy_version='v3' AND status IN ('proposed','unclassified') ORDER BY document_id, created_at DESC, id DESC").fetch_all(&mut **tx).await?;
    let mut selected = HashMap::new();
    let mut seen = BTreeSet::new();
    let mut statuses = HashMap::new();
    for row in runs {
        let id: i64 = row.try_get("document_id")?;
        let hash: String = row.try_get("content_hash")?;
        if hashes.get(&id) == Some(&hash) && seen.insert(id) {
            let status: String = row.try_get("status")?;
            if status == "proposed" {
                selected.insert(row.try_get::<i64, _>("id")?, id);
            }
            statuses.insert(id, status);
        }
    }
    let mut topics: HashMap<i64, BTreeMap<String, String>> = HashMap::new();
    for row in sqlx::query("SELECT run_id, topic_id FROM classification_topics")
        .fetch_all(&mut **tx)
        .await?
    {
        if let Some(id) = selected.get(&row.try_get::<i64, _>("run_id")?) {
            topics
                .entry(*id)
                .or_default()
                .insert(row.try_get("topic_id")?, "unreviewed".into());
        }
    }
    let reviews = sqlx::query("SELECT DISTINCT ON (document_id, topic_id) document_id, content_hash, topic_id, decision FROM classification_review_events WHERE taxonomy_version='v3' ORDER BY document_id, topic_id, id DESC").fetch_all(&mut **tx).await?;
    for row in reviews {
        let id: i64 = row.try_get("document_id")?;
        let hash: String = row.try_get("content_hash")?;
        if hashes.get(&id) != Some(&hash) {
            continue;
        }
        let topic: String = row.try_get("topic_id")?;
        let decision: String = row.try_get("decision")?;
        if decision == "rejected" {
            topics.entry(id).or_default().remove(&topic);
        } else {
            topics.entry(id).or_default().insert(topic, decision);
        }
    }
    let mut result = HashMap::new();
    for id in statuses
        .keys()
        .chain(topics.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let entries = topics.remove(&id).unwrap_or_default();
        let status = if !entries.is_empty() {
            "proposed"
        } else {
            statuses.get(&id).map(String::as_str).unwrap_or("missing")
        };
        let mut values = vec![format!("__status:{status}")];
        for (topic, review) in entries {
            values.push(format!("__review:{topic}:{review}"));
            values.push(topic);
        }
        result.insert(id, values);
    }
    Ok(result)
}
