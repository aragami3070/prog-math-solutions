use rust_knowledge::{
    api::{self, AppState},
    config::Config,
    crawler::sources::parse,
    models::{
        crawls::{
            CollectedPage, CrawlPlan, CrawlRun, FetchKind, FetchRecord, QueueLink, RunStatus,
        },
        pagination::Pagination,
        sources::{Completeness, Source},
        statistics::Statistics,
    },
    use_cases::{crawls, sources, statistics},
};
use sqlx::{Connection, PgPool, Postgres, pool::PoolConnection};

const URL: &str = "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html";
const HTML: &str = include_str!("fixtures/rfc.html");

fn plan(source: Source) -> CrawlPlan {
    CrawlPlan {
        source,
        user_agent: "fixture-only".into(),
        delay: std::time::Duration::from_secs(5),
        fresh_for: std::time::Duration::from_secs(3600),
    }
}

// Mirrors the router's boundary without dispatching any external crawl.
async fn start_run(
    pool: &PgPool,
    source: Source,
) -> Result<(CrawlRun, PoolConnection<Postgres>), crawls::errors::Error> {
    let mut guard = pool.acquire().await.unwrap();
    guard.close_on_drop();
    let run = {
        let mut tx = guard.begin().await.unwrap();
        let run = crawls::start::execute(&mut tx, &plan(source)).await?;
        tx.commit().await.unwrap();
        run
    };
    Ok((run, guard))
}

async fn stats(pool: &PgPool) -> Statistics {
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
        .unwrap();
    let result = statistics::get::execute(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    result
}

async fn recover(pool: &PgPool) {
    let mut connection = pool.acquire().await.unwrap();
    connection.close_on_drop();
    let mut tx = connection.begin().await.unwrap();
    crawls::recover::execute(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    connection.close().await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn collection_lifecycle(pool: PgPool) {
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
        .unwrap();
    let empty = sources::list_documents::execute(
        &mut tx,
        Pagination {
            page: 1,
            page_size: 1,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!((empty.total, empty.total_pages), (0, 0));
    assert_eq!(stats(&pool).await.sources.len(), 2);

    let (run, guard) = start_run(&pool, Source::Rfc).await.unwrap();
    assert!(matches!(
        start_run(&pool, Source::Internals).await,
        Err(crawls::errors::Error::Busy)
    ));
    recover(&pool).await;
    assert_eq!(stats(&pool).await.recent_runs[0].status, RunStatus::Running);
    for _ in 0..2 {
        let mut tx = pool.begin().await.unwrap();
        let fetch_id = crawls::record_fetch::execute(
            &mut tx,
            FetchRecord {
                run_id: run.id,
                url: URL,
                kind: FetchKind::Document,
                status: Some(200),
                content_type: Some("text/html"),
                body: Some(HTML),
                error: None,
            },
        )
        .await
        .unwrap();
        crawls::save_document::execute(
            &mut tx,
            run.id,
            Source::Rfc,
            fetch_id,
            &parse(Source::Rfc, URL, HTML).unwrap(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    crawls::finish::execute(&mut tx, run.id, Source::Rfc, RunStatus::Completed, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    guard.close().await.unwrap();

    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
        .unwrap();
    let page = sources::list_documents::execute(
        &mut tx,
        Pagination {
            page: 1,
            page_size: 1,
        },
    )
    .await
    .unwrap();
    let second = sources::list_documents::execute(
        &mut tx,
        Pagination {
            page: 2,
            page_size: 1,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!((page.total, page.total_pages, page.items.len()), (1, 1, 1));
    assert_eq!(page.items[0].native_id, "3058");
    assert_eq!(page.items[0].completeness, Completeness::Complete);
    assert!(second.items.is_empty());
    assert_eq!(second.total, 1);
    let report = stats(&pool).await;
    assert_eq!(
        (
            report.total_documents,
            report.document_requests,
            report.http_errors
        ),
        (1, 2, 0)
    );
    assert_eq!(report.recent_runs[0].succeeded, 2);
    let raw_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM fetches
        WHERE body IS NOT NULL
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw_count, 2);

    let (blocked, guard) = start_run(&pool, Source::Internals).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    crawls::finish::execute(
        &mut tx,
        blocked.id,
        Source::Internals,
        RunStatus::Blocked,
        Some("fixture 429"),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    guard.close().await.unwrap();
    assert!(matches!(
        start_run(&pool, Source::Internals).await,
        Err(crawls::errors::Error::Paused(_))
    ));

    // Real routers use the same use cases and commit source changes.
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let config = Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        web_dist: "web/dist".into(),
        crawler_user_agent: None,
        crawl_delay: std::time::Duration::from_secs(5),
        fresh_for: std::time::Duration::from_secs(3600),
    };
    let app = api::router(AppState::new(pool.clone(), &config), &config.web_dist);
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/sources/internals/resume")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/documents?page=1&page_size=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(value["total"], 1);
    let response = app
        .oneshot(Request::get("/api/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (abandoned, guard) = start_run(&pool, Source::Internals).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: abandoned.id,
            url: "fixture",
            kind: FetchKind::Document,
            status: Some(200),
            content_type: None,
            body: None,
            error: Some("size limit"),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    guard.close().await.unwrap();
    recover(&pool).await;
    let report = stats(&pool).await;
    assert_eq!(report.recent_runs[0].status, RunStatus::Interrupted);
    assert_eq!(report.http_errors, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn caller_controls_commit_and_rollback(pool: PgPool) {
    let (run, guard) = start_run(&pool, Source::Rfc).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let fetch_id = crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: run.id,
            url: URL,
            kind: FetchKind::Document,
            status: Some(200),
            content_type: Some("text/html"),
            body: Some(HTML),
            error: None,
        },
    )
    .await
    .unwrap();
    crawls::save_document::execute(
        &mut tx,
        run.id,
        Source::Rfc,
        fetch_id,
        &parse(Source::Rfc, URL, HTML).unwrap(),
    )
    .await
    .unwrap();
    crawls::finish::execute(
        &mut tx,
        run.id,
        Source::Rfc,
        RunStatus::Blocked,
        Some("fixture stop"),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();

    let report = stats(&pool).await;
    assert_eq!((report.total_documents, report.document_requests), (0, 0));
    assert_eq!(report.recent_runs[0].succeeded, 0);
    assert_eq!(report.recent_runs[0].status, RunStatus::Running);
    assert!(report.sources.iter().all(|s| !s.paused));

    let mut tx = pool.begin().await.unwrap();
    crawls::finish::execute(
        &mut tx,
        run.id,
        Source::Rfc,
        RunStatus::Blocked,
        Some("fixture stop"),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let report = stats(&pool).await;
    assert_eq!(report.recent_runs[0].status, RunStatus::Blocked);
    assert!(
        report
            .sources
            .iter()
            .find(|s| s.source == "rfc")
            .unwrap()
            .paused
    );
    guard.close().await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn crawl_frontier_checkpoints_deduplicates_and_resumes(pool: PgPool) {
    let (run, guard) = start_run(&pool, Source::Rfc).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let seed = crawls::next_page::execute(&mut tx, run.id)
        .await
        .unwrap()
        .unwrap();
    let page = || CollectedPage {
        id: seed.id,
        fetch_id: 0, // Navigation pages do not create a document.
        parsed: None,
        links: vec![
            QueueLink::same(seed.url.clone()),
            QueueLink::same(URL),
            QueueLink::same(URL),
        ],
    };
    crawls::complete_page::execute(&mut tx, run.id, Source::Rfc, page())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let report = stats(&pool).await;
    assert_eq!(
        (
            report.recent_runs[0].requested,
            report.recent_runs[0].succeeded
        ),
        (1, 0)
    );
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        crawls::next_page::execute(&mut tx, run.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        seed.id
    );
    crawls::complete_page::execute(&mut tx, run.id, Source::Rfc, page())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let report = stats(&pool).await;
    assert_eq!(
        (
            report.recent_runs[0].requested,
            report.recent_runs[0].succeeded
        ),
        (2, 1)
    );

    let mut tx = pool.begin().await.unwrap();
    let doc_page = crawls::next_page::execute(&mut tx, run.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(doc_page.url, URL);
    crawls::fail_page::execute(&mut tx, run.id, doc_page.id, "fixture timeout", &[])
        .await
        .unwrap();
    crawls::finish::execute(&mut tx, run.id, Source::Rfc, RunStatus::Failed, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    guard.close().await.unwrap();

    let (resumed, guard) = start_run(&pool, Source::Rfc).await.unwrap();
    assert_eq!(resumed.id, run.id);
    assert_eq!(
        (resumed.requested, resumed.succeeded, resumed.failed),
        (2, 1, 0)
    );
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        crawls::next_page::execute(&mut tx, run.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        doc_page.id
    );
    let fetch_id = crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: run.id,
            url: URL,
            kind: FetchKind::Document,
            status: Some(200),
            content_type: Some("text/html"),
            body: Some(HTML),
            error: None,
        },
    )
    .await
    .unwrap();
    crawls::complete_page::execute(
        &mut tx,
        run.id,
        Source::Rfc,
        CollectedPage {
            id: doc_page.id,
            fetch_id,
            parsed: Some(parse(Source::Rfc, URL, HTML).unwrap()),
            links: vec![QueueLink::same(seed.url), QueueLink::same(URL)],
        },
    )
    .await
    .unwrap();
    assert!(
        crawls::next_page::execute(&mut tx, run.id)
            .await
            .unwrap()
            .is_none()
    );
    crawls::finish::execute(&mut tx, run.id, Source::Rfc, RunStatus::Completed, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    guard.close().await.unwrap();
    let report = stats(&pool).await;
    assert_eq!(report.total_documents, 1);
    assert_eq!(
        (
            report.recent_runs[0].requested,
            report.recent_runs[0].succeeded
        ),
        (2, 2)
    );
    let (fresh, guard) = start_run(&pool, Source::Rfc).await.unwrap();
    assert_ne!(fresh.id, run.id);
    guard.close().await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test role with CREATEDB; see README"]
async fn queue_requests_the_slug_url_but_deduplicates_by_identity(pool: PgPool) {
    let (run, guard) = start_run(&pool, Source::Internals).await.unwrap();
    let topic = "https://internals.rust-lang.org/t/42";
    let slugged = "https://internals.rust-lang.org/t/the-slug/42";
    let link = |fetch_url: &str| QueueLink {
        url: topic.into(),
        fetch_url: fetch_url.into(),
    };
    let mut tx = pool.begin().await.unwrap();
    let seed = crawls::next_page::execute(&mut tx, run.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seed.fetch_url, seed.url);
    // A slug-less sighting first, then the same page with a slug, then a different slug.
    for fetch_url in [
        topic,
        slugged,
        "https://internals.rust-lang.org/t/old-slug/42",
    ] {
        crawls::complete_page::execute(
            &mut tx,
            run.id,
            Source::Internals,
            CollectedPage {
                id: seed.id,
                fetch_id: 0,
                parsed: None,
                links: vec![link(fetch_url)],
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let rows = sqlx::query!(
        r#"
        SELECT url, fetch_url
        FROM crawl_queue
        WHERE run_id = $1 AND url = $2
        "#,
        run.id,
        topic,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fetch_url, slugged);
    // Both sightings count once: the seed and the topic.
    assert_eq!(stats(&pool).await.recent_runs[0].requested, 2);
    guard.close().await.unwrap();
}
