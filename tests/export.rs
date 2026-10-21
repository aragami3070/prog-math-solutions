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
        crawls::FetchRecord,
        exports::{ExportDocument, ExportFormat, ExportQuery},
        sources::Source,
    },
    repository,
    use_cases::{crawls, sources},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    io::{Cursor, Read},
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tower::ServiceExt;

fn state(pool: PgPool) -> (AppState, Config) {
    let config = Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        web_dist: "web/dist".into(),
        crawler_user_agent: None,
        crawl_delay: Duration::from_secs(5),
    };
    (AppState::new(pool, &config), config)
}

async fn insert(pool: &PgPool, source: Source, id: usize) {
    let (url, html) = match source {
        Source::Rfc => (
            format!("https://rust-lang.github.io/rfcs/{id:04}-example.html"),
            include_str!("fixtures/rfc.html"),
        ),
        Source::Internals => (
            format!("https://internals.rust-lang.org/t/{id}"),
            include_str!("fixtures/internals.html"),
        ),
    };
    let mut tx = pool.begin().await.unwrap();
    let run = repository::crawls::insert(&mut tx, source, 1)
        .await
        .unwrap();
    let fetch_id = crawls::record_fetch::execute(
        &mut tx,
        FetchRecord {
            run_id: run.id,
            url: &url,
            kind: "document",
            status: Some(200),
            content_type: Some("text/html"),
            body: Some(html),
            error: None,
        },
    )
    .await
    .unwrap();
    let mut doc = parse(source, &url, html).unwrap();
    doc.title = "=formula, \"quoted\"".into();
    doc.posts[0]
        .text
        .push_str("\nКириллица, \"кавычки\"\r\nnew line");
    crawls::save_document::execute(&mut tx, run.id, source, fetch_id, &doc)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn download(app: &axum::Router, query: &str) -> (axum::http::HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/sources/export?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    assert_eq!(
        headers["content-length"]
            .to_str()
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        bytes.len()
    );
    assert_eq!(
        headers["x-content-sha256"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert_eq!(headers["cache-control"], "no-store");
    (headers, bytes)
}

fn zip_files(bytes: Vec<u8>) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut files = std::collections::BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(name, bytes);
    }
    files
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL with CREATEDB"]
async fn export_formats_filters_checksums_and_snapshot(pool: PgPool) {
    let (state, config) = state(pool.clone());
    let slots = state.export_slots.clone();
    let app = api::router(state, &config.web_dist);
    for format in ["json", "jsonl", "csv", "zip"] {
        let (headers, bytes) = download(&app, &format!("format={format}")).await;
        assert_eq!(headers["x-corpus-documents"], "0");
        match format {
            "json" => assert_eq!(
                serde_json::from_slice::<Vec<ExportDocument>>(&bytes)
                    .unwrap()
                    .len(),
                0
            ),
            "jsonl" => assert!(bytes.is_empty()),
            "csv" => assert_eq!(
                csv::Reader::from_reader(bytes.as_slice()).records().count(),
                0
            ),
            _ => assert_eq!(zip_files(bytes).len(), 4),
        }
    }
    // More than two SQL batches; insertion order is deliberately reversed.
    for id in (1..=65).rev() {
        insert(&pool, Source::Rfc, id).await;
    }
    insert(&pool, Source::Internals, 42).await;
    let (_, json) = download(&app, "format=json").await;
    let docs: Vec<ExportDocument> = serde_json::from_slice(&json).unwrap();
    assert_eq!(docs.len(), 66);
    assert!(docs.windows(2).all(|w| w[0].document_id < w[1].document_id));
    assert_eq!(docs[0].document_id, "internals:42");
    let (_, jsonl) = download(&app, "format=jsonl").await;
    let lines: Vec<ExportDocument> = std::str::from_utf8(&jsonl)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(docs, lines);
    let (_, csv) = download(&app, "format=csv").await;
    let mut reader = csv::Reader::from_reader(csv.as_slice());
    let headers = reader.headers().unwrap().clone();
    let body_col = headers.iter().position(|s| s == "body_text").unwrap();
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows.len(), 66);
    for (row, doc) in rows.iter().zip(&docs) {
        assert_eq!(&row[body_col], doc.body_text);
    }
    let (_, safe) = download(&app, "format=csv&spreadsheet_safe=true").await;
    assert!(
        csv::Reader::from_reader(safe.as_slice())
            .records()
            .next()
            .unwrap()
            .unwrap()[5]
            .starts_with("'=")
    );
    let (_, zip) = download(&app, "format=zip").await;
    let files = zip_files(zip);
    assert_eq!(files["corpus.json"], json);
    assert_eq!(files["corpus.jsonl"], jsonl);
    assert_eq!(files["corpus.csv"], csv);
    let manifest: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(manifest["documents"], 66);
    assert_eq!(manifest["documents_by_source"]["rfc"], 65);
    assert_eq!(
        manifest["corpus_sha256"],
        format!("{:x}", Sha256::digest(&jsonl))
    );
    for name in ["corpus.json", "corpus.jsonl", "corpus.csv"] {
        assert_eq!(
            manifest["files"][name]["sha256"],
            format!("{:x}", Sha256::digest(&files[name]))
        );
    }
    for query in ["format=json&source=rfc", "format=json&complete_only=true"] {
        let (headers, _) = download(&app, query).await;
        assert_eq!(headers["x-corpus-documents"], "65");
    }
    let (_, empty) = download(&app, "format=json&source=internals&complete_only=true").await;
    assert_eq!(empty, b"[]\n");

    let permit = slots.acquire().await.unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/sources/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    drop(permit);

    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
        .unwrap();
    repository::sources::export_snapshot_time(&mut tx)
        .await
        .unwrap();
    insert(&pool, Source::Rfc, 99).await;
    insert(&pool, Source::Rfc, 1).await;
    let mut artifact = sources::export_corpus::execute(
        &mut tx,
        &ExportQuery {
            format: ExportFormat::Json,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(artifact.count, 66);
    let mut bytes = Vec::new();
    artifact.file.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, json);
    let (headers, _) = download(&app, "format=json").await;
    assert_eq!(headers["x-corpus-documents"], "67");
}
