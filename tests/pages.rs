use rust_knowledge::{
    crawler::sources::parse,
    models::{crawls::FetchRecord, sources::Source},
    repository,
    use_cases::{crawls, sources::reparse},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const TOPIC: &str = "https://internals.rust-lang.org/t/example/42";

/// A Discourse-like crawler page. Post N carries a link `https://example.org/N`.
fn page_html(posts: &[i32], has_next: bool) -> String {
    let next = if has_next {
        r#"<link rel="next" href="/t/example/42?page=9">"#
    } else {
        ""
    };
    let posts: String = posts
        .iter()
        .map(|n| {
            format!(
                r#"<div class="crawler-post" id="post_{n}"><div class="creator"><span itemprop="name">user{n}</span><time datetime="2026-10-01T12:00:0{n}Z"></time></div>
<div class="post" itemprop="articleBody"><p>Post number {n} discusses Rust traits and compiler internals at length. <a href="https://example.org/{n}">link {n}</a></p></div></div>"#
            )
        })
        .collect();
    format!("<html><head>{next}</head><body><h1>Example discussion</h1>{posts}</body></html>")
}

fn url(page_no: u32) -> String {
    match page_no {
        1 => TOPIC.to_owned(),
        n => format!("{TOPIC}?page={n}"),
    }
}

/// Stores a response in `fetches`, like the transport does, and returns its ID.
async fn fetch(pool: &PgPool, run_id: i64, url: &str, html: &str) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    let id = crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id,
            url,
            kind: "document",
            status: Some(200),
            content_type: Some("text/html"),
            body: Some(html),
            error: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn new_run(pool: &PgPool) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    let run = repository::crawls::insert(&mut tx, Source::Internals, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    run.id
}

async fn save(pool: &PgPool, run_id: i64, page_no: u32, posts: &[i32], has_next: bool) {
    let html = page_html(posts, has_next);
    let url = url(page_no);
    let fetch_id = fetch(pool, run_id, &url, &html).await;
    let page = parse(Source::Internals, &url, &html).unwrap();
    let mut tx = pool.begin().await.unwrap();
    crawls::save_document::execute(&mut tx, run_id, Source::Internals, fetch_id, &page)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

struct Doc {
    body_text: String,
    content_hash: String,
    completeness: String,
    reason: String,
}

async fn doc(pool: &PgPool) -> Doc {
    let row = sqlx::query!(
        r#"
        SELECT body_text, content_hash, completeness, completeness_reason
        FROM documents
        WHERE source = 'internals' AND native_id = '42'
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    Doc {
        body_text: row.body_text,
        content_hash: row.content_hash,
        completeness: row.completeness,
        reason: row.completeness_reason,
    }
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn pages_accumulate_across_runs_and_completeness_follows_the_chain(pool: PgPool) {
    let first = new_run(&pool).await;
    save(&pool, first, 1, &[1, 2], true).await;
    let d = doc(&pool).await;
    assert_eq!(d.completeness, "partial");
    assert!(d.reason.contains("links to a next page"), "{}", d.reason);

    save(&pool, first, 3, &[5, 6], false).await;
    let d = doc(&pool).await;
    assert_eq!(d.completeness, "partial");
    assert!(d.reason.contains("Page 2"), "{}", d.reason);

    save(&pool, first, 2, &[3, 4], true).await;
    let d = doc(&pool).await;
    assert_eq!(
        (d.completeness.as_str(), count(&pool, "posts").await),
        ("complete", 6)
    );
    // Posts come back in thread order regardless of the order pages were fetched.
    let order: Vec<usize> = (1..=6)
        .map(|n| d.body_text.find(&format!("Post number {n} ")).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        d.content_hash,
        format!("{:x}", Sha256::digest(d.body_text.as_bytes()))
    );

    // A later run that only reaches page 1 must not shrink the document.
    let second = new_run(&pool).await;
    save(&pool, second, 1, &[1, 2], true).await;
    let d = doc(&pool).await;
    assert_eq!(
        (d.completeness.as_str(), count(&pool, "posts").await),
        ("complete", 6)
    );
    assert!(d.body_text.contains("Post number 6 "));
    assert_eq!(count(&pool, "document_pages").await, 3);
    assert_eq!(count(&pool, "documents").await, 1);

    // A refetched page replaces its own posts and links; a deleted post disappears.
    assert_eq!(count(&pool, "links").await, 6);
    save(&pool, second, 2, &[3], true).await;
    let d = doc(&pool).await;
    assert!(!d.body_text.contains("Post number 4 "));
    assert_eq!(
        (count(&pool, "posts").await, count(&pool, "links").await),
        (5, 5)
    );
    let anchor = sqlx::query_scalar!(
        r#"
        SELECT l.anchor_text
        FROM links AS l
        JOIN posts AS p ON p.id = l.post_id
        WHERE p.post_number = 3 AND l.target_url = 'https://example.org/3'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anchor.as_deref(), Some("link 3"));

    // A post that moved to another page is reassigned, not duplicated.
    save(&pool, second, 3, &[4, 5, 6], false).await;
    assert_eq!(count(&pool, "posts").await, 6);
    assert_eq!(doc(&pool).await.completeness, "complete");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn reparse_rebuilds_from_stored_responses_and_is_idempotent(pool: PgPool) {
    let run = new_run(&pool).await;
    fetch(&pool, run, &url(1), &page_html(&[1, 2], true)).await;
    // Later fetch of the same page wins.
    fetch(&pool, run, &url(2), &page_html(&[3], false)).await;
    fetch(&pool, run, &url(2), &page_html(&[3, 4], false)).await;
    // Navigation pages and redirects carry no document.
    fetch(
        &pool,
        run,
        "https://internals.rust-lang.org/latest?page=2",
        &page_html(&[1], false),
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: run,
            url: "https://internals.rust-lang.org/t/42",
            kind: "document",
            status: Some(301),
            content_type: None,
            body: None,
            error: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut results = Vec::new();
    for _ in 0..2 {
        let mut tx = pool.begin().await.unwrap();
        let ids = reparse::fetch_ids(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(ids.len(), 4);
        let mut outcomes = Vec::new();
        for id in ids {
            let mut tx = pool.begin().await.unwrap();
            outcomes.push(reparse::execute(&mut tx, id).await.unwrap());
            tx.commit().await.unwrap();
        }
        assert_eq!(
            outcomes
                .iter()
                .filter(|o| **o == reparse::Outcome::Saved)
                .count(),
            3
        );
        let d = doc(&pool).await;
        results.push((d.body_text, d.content_hash, d.completeness));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].2, "complete");
    assert_eq!(
        (
            count(&pool, "posts").await,
            count(&pool, "document_pages").await
        ),
        (4, 2)
    );
    assert_eq!(count(&pool, "documents").await, 1);
}
