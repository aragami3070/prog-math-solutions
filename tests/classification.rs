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

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn effective_v3_respects_hash_status_and_reviews(pool: PgPool) {
    let id: i64 = sqlx::query_scalar("INSERT INTO documents (source,native_id,canonical_url,title,body_text,content_hash,completeness,completeness_reason,metadata,parser_version) VALUES ('rfc','test','https://example.com','test','body','other','complete','test','{}','test') RETURNING id").fetch_one(&pool).await.unwrap();
    let run: i64 = sqlx::query_scalar(
        "INSERT INTO crawl_runs(source,status,requested) VALUES ('rfc','completed',1) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let fetch: i64 = sqlx::query_scalar("INSERT INTO fetches(run_id,url,kind) VALUES ($1,'https://example.com','document') RETURNING id").bind(run).fetch_one(&pool).await.unwrap();
    let page: i64 = sqlx::query_scalar("INSERT INTO document_pages(document_id,page_no,url,fetch_id,has_next) VALUES ($1,1,'https://example.com',$2,false) RETURNING id").bind(id).bind(fetch).fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO posts(document_id,page_id,post_number,text) VALUES ($1,$2,1,'body')")
        .bind(id)
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    let hash = "41442492a487b1d4f54e608ae217e2dd7aefc743bd2c8a7253895011fe8569df";
    let class: i64 = sqlx::query_scalar("INSERT INTO classification_runs(document_id,content_hash,taxonomy_version,pipeline_version,status,provenance) VALUES ($1,$2,'v3','test','proposed','{}') RETURNING id").bind(id).bind(hash).fetch_one(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO classification_topics(run_id,topic_id,evidence) VALUES ($1,'lifetimes','[]')",
    )
    .bind(class)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO classification_review_events(document_id,taxonomy_version,content_hash,topic_id,decision,reviewer) VALUES ($1,'v3',$2,'lifetimes','rejected','test'),($1,'v3',$2,'macros','added','test')").bind(id).bind(hash).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let snapshot = rust_knowledge::repository::search::effective_topics(&mut tx)
        .await
        .unwrap();
    let topics = &snapshot[&id];
    assert!(topics.contains(&"macros".to_owned()));
    assert!(topics.contains(&"__review:macros:added".to_owned()));
    assert!(!topics.contains(&"lifetimes".to_owned()));
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE posts SET text='changed' WHERE document_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        !rust_knowledge::repository::search::effective_topics(&mut tx)
            .await
            .unwrap()
            .contains_key(&id)
    );
}
