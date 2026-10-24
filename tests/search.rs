use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use rust_knowledge::{
    api::{self, AppState},
    config::Config,
    crawler::sources::parse,
    models::{
        crawls::{FetchKind, FetchRecord},
        sources::Source,
    },
    repository,
    use_cases::crawls,
};
use serde_json::Value;
use sqlx::PgPool;
use std::time::Duration;
use tower::ServiceExt;

fn app(pool: PgPool, index_dir: &std::path::Path) -> axum::Router {
    let config = Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        web_dist: "web/dist".into(),
        crawler_user_agent: None,
        crawl_delay: Duration::from_secs(5),
        fresh_for: Duration::from_secs(3600),
        search_dir: index_dir.to_owned(),
    };
    api::router(AppState::new(pool, &config), &config.web_dist)
}

async fn insert(pool: &PgPool, source: Source, url: &str, html: &str) {
    let mut tx = pool.begin().await.unwrap();
    let run = repository::crawls::insert(&mut tx, source, 1)
        .await
        .unwrap();
    let fetch_id = crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: run.id,
            url,
            kind: FetchKind::Document,
            status: Some(200),
            content_type: Some("text/html"),
            body: Some(html),
            error: None,
        },
    )
    .await
    .unwrap();
    let doc = parse(source, url, html).unwrap();
    crawls::save_document::execute(&mut tx, run.id, source, fetch_id, &doc)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn call(app: &axum::Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL with CREATEDB"]
async fn search_needs_a_built_index_then_finds_ids_text_and_applies_filters(pool: PgPool) {
    insert(
        &pool,
        Source::Rfc,
        "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html",
        include_str!("fixtures/rfc.html"),
    )
    .await;
    insert(
        &pool,
        Source::Internals,
        "https://internals.rust-lang.org/t/7",
        include_str!("fixtures/internals.html"),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let app = app(pool.clone(), dir.path());

    let (status, body) = call(&app, "GET", "/api/search?q=try").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"].as_str().unwrap().contains("rebuild"));

    let (status, built) = call(&app, "POST", "/api/search/rebuild").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(built["documents"], 2);

    let (status, found) = call(&app, "GET", "/api/search?q=RFC%203058").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["hits"][0]["document_id"], "rfc:3058");
    assert_eq!(found["scope"]["mode"], "lexical");
    assert_eq!(found["scope"]["generation"], built["generation"]);
    assert_eq!(found["total"], found["hits"].as_array().unwrap().len());

    let (_, found) = call(&app, "GET", "/api/search?q=try&source=internals").await;
    assert!(
        found["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["source"] == "internals")
    );

    // The snippet is plain parts, so a page can show it without trusting HTML.
    let (_, found) = call(&app, "GET", "/api/search?q=3058").await;
    let snippet = found["hits"][0]["snippet"].as_array().unwrap();
    assert!(
        snippet
            .iter()
            .all(|p| p["text"].is_string() && p["matched"].is_boolean())
    );

    // Rebuilding keeps working and moves to a new generation.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let (_, again) = call(&app, "POST", "/api/search/rebuild").await;
    assert_ne!(again["generation"], built["generation"]);

    for query in [
        "q=",
        "q=%20%20",
        "q=try&page=0",
        "q=try&page_size=101",
        "q=try&page=11&page_size=100",
        "q=try&bogus=1",
        "page=1",
        "q=try&source=nope",
    ] {
        let (status, _) = call(&app, "GET", &format!("/api/search?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
    }
}
