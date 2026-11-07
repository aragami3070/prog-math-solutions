use rust_knowledge::use_cases::graph::overview;
use sqlx::PgPool;

/// Three documents in a chain a→b→c and one with no edges: the overview lists the linked three
/// (most-linked first), both edges, and leaves the isolated document out.
#[sqlx::test(migrations = "./migrations")]
async fn overview_lists_linked_documents_only(pool: PgPool) {
    let mut ids = Vec::new();
    for n in 1..=4 {
        ids.push(
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO documents (source, native_id, canonical_url, title, body_text, content_hash, completeness, completeness_reason, metadata, parser_version)
                 VALUES ('internals', $1, $2, $3, 'text', $1, 'complete', 'fixture', '{}', 'v') RETURNING id",
            )
            .bind(n.to_string())
            .bind(format!("https://internals.rust-lang.org/t/{n}"))
            .bind(format!("Topic {n}"))
            .fetch_one(&pool)
            .await
            .unwrap(),
        );
    }
    for (from, to) in [(0, 1), (1, 2)] {
        sqlx::query("INSERT INTO edges (source_id, target_id, kind, occurrences) VALUES ($1, $2, 'references', 1)")
            .bind(ids[from])
            .bind(ids[to])
            .execute(&pool)
            .await
            .unwrap();
    }

    let mut tx = pool.begin().await.unwrap();
    let graph = overview::execute(&mut tx).await.unwrap();
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.nodes[0].id, "internals:2");
    assert_eq!(graph.nodes[0].degree, 2);
    assert!(graph.nodes.iter().all(|node| node.id != "internals:4"));
    assert_eq!(graph.edges.len(), 2);
    assert!(!graph.truncated);
}
