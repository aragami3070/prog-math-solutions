use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn classification_storage_preserves_manual_decisions(pool: PgPool) {
    let document: i64 = sqlx::query_scalar("INSERT INTO documents (source, native_id, canonical_url, title, body_text, content_hash, completeness, completeness_reason, metadata, parser_version) VALUES ('rfc', 'test', 'https://example.com', 'test', 'body', 'hash1', 'complete', 'test', '{}', 'test') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let insert = "INSERT INTO classification_runs (document_id, content_hash, taxonomy_version, pipeline_version, status, provenance, coverage) VALUES ($1, $2, 'v1', 'p1', 'proposed', '{\"model\":\"fixture\"}', 1) RETURNING id";
    let first: i64 = sqlx::query_scalar(insert)
        .bind(document)
        .bind("hash1")
        .fetch_one(&pool)
        .await
        .unwrap();
    for topic in ["lifetimes", "traits-generics"] {
        sqlx::query("INSERT INTO classification_topics (run_id, topic_id, evidence) VALUES ($1, $2, '[{\"section_id\":\"1\",\"quote\":\"body\"}]')")
            .bind(first).bind(topic).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO classification_review_events (document_id, taxonomy_version, content_hash, topic_id, decision, reviewer) VALUES ($1, 'v1', 'hash1', 'lifetimes', 'rejected', 'reviewer')")
        .bind(document).execute(&pool).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, i64>(insert)
            .bind(document)
            .bind("hash1")
            .fetch_one(&pool)
            .await
            .is_err()
    );
    let second: i64 = sqlx::query_scalar(insert)
        .bind(document)
        .bind("hash2")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO classification_topics (run_id, topic_id, evidence) VALUES ($1, 'lifetimes', '[]')")
        .bind(second).execute(&pool).await.unwrap();
    let decision: String = sqlx::query_scalar("SELECT decision FROM classification_review_events WHERE document_id = $1 AND taxonomy_version = 'v1' AND topic_id = 'lifetimes' ORDER BY id DESC LIMIT 1")
        .bind(document).fetch_one(&pool).await.unwrap();
    assert_eq!(decision, "rejected");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM classification_topics WHERE run_id = $1")
            .bind(first)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
    assert!(sqlx::query("INSERT INTO classification_runs (document_id, content_hash, taxonomy_version, pipeline_version, status, provenance, coverage) VALUES ($1, 'hash3', 'v1', 'p1', 'reviewed', '{}', 2)")
        .bind(document).execute(&pool).await.is_err());
    assert!(sqlx::query("INSERT INTO classification_review_events (document_id, taxonomy_version, content_hash, topic_id, decision, reviewer) VALUES (9223372036854775807, 'v1', 'hash1', 'lifetimes', 'accepted', 'reviewer')")
        .execute(&pool).await.is_err());
}
