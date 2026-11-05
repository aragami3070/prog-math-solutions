use crate::models::graph::{EdgeRow, NodeRow};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

/// Rebuilds every `references` edge from the stored links. A link points at a document when its
/// URL (fragment and slug aside) names one: `/t/<slug>/<id>` or `/t/<id>` on Internals,
/// `rfcs/<nnnn>-<slug>.html` or the GitHub `text/<nnnn>-<slug>.md` for RFCs. Links to a
/// document from itself (footnotes, post anchors) are not edges.
pub async fn rebuild_references(tx: &mut Transaction<'_, Postgres>) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM edges WHERE kind = 'references'")
        .execute(&mut **tx)
        .await?;
    let inserted = sqlx::query!(
        r#"
        INSERT INTO edges (source_id, target_id, kind, occurrences, evidence)
        SELECT
            l.document_id, t.id, 'references', COUNT(*)::INTEGER,
            to_jsonb((array_agg(
                jsonb_build_object('post_id', l.post_id, 'url', l.target_url, 'anchor_text', l.anchor_text)
                ORDER BY l.post_id, l.id
            ))[1:10])
        FROM links AS l
        JOIN documents AS t ON (t.source || ':' || t.native_id) = COALESCE(
            (SELECT 'internals:' || m[1] FROM regexp_match(
                l.target_url,
                '^https://internals\.rust-lang\.org/t/(?:[^/#?]*[^0-9/#?][^/#?]*/)?([0-9]+)') AS m),
            (SELECT 'rfc:' || ltrim(m[1], '0') FROM regexp_match(
                l.target_url, '^https://rust-lang\.github\.io/rfcs/([0-9]{4})-[^/#?]*\.html') AS m),
            (SELECT 'rfc:' || ltrim(m[1], '0') FROM regexp_match(
                l.target_url, '^https://github\.com/rust-lang/rfcs/blob/[^/]+/text/([0-9]{4})-') AS m)
        )
        WHERE t.id <> l.document_id
        GROUP BY l.document_id, t.id
        "#
    )
    .execute(&mut **tx)
    .await?;
    Ok(inserted.rows_affected())
}

pub async fn node(
    tx: &mut Transaction<'_, Postgres>,
    document: &str,
) -> Result<Option<NodeRow>, sqlx::Error> {
    sqlx::query_as!(
        NodeRow,
        r#"
        SELECT id, source, native_id, title, canonical_url, completeness AS "completeness!"
        FROM documents WHERE (source || ':' || native_id) = $1
        "#,
        document
    )
    .fetch_optional(&mut **tx)
    .await
}

pub async fn nodes(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
) -> Result<Vec<NodeRow>, sqlx::Error> {
    sqlx::query_as!(
        NodeRow,
        r#"
        SELECT id, source, native_id, title, canonical_url, completeness AS "completeness!"
        FROM documents WHERE id = ANY($1)
        "#,
        ids
    )
    .fetch_all(&mut **tx)
    .await
}

/// Edges touching a document, most-linked first.
pub async fn edges_of(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<Vec<EdgeRow>, sqlx::Error> {
    sqlx::query_as!(
        EdgeRow,
        r#"
        SELECT source_id, target_id, occurrences, evidence
        FROM edges
        WHERE kind = 'references' AND (source_id = $1 OR target_id = $1)
        ORDER BY occurrences DESC, id
        "#,
        id
    )
    .fetch_all(&mut **tx)
    .await
}

pub async fn built_at(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT MAX(built_at) FROM edges"#)
        .fetch_one(&mut **tx)
        .await
}
