use super::*;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn app() -> Router {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .unwrap();
    let config = Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        web_dist: "web/dist".into(),
        crawler_user_agent: None,
        crawl_delay: std::time::Duration::from_secs(5),
        fresh_for: std::time::Duration::from_secs(3600),
        search_dir: std::env::temp_dir().join("rks-test-index"),
        search_refresh: None,
    };
    router(AppState::new(pool, &config), &config.web_dist)
}

#[tokio::test]
async fn taxonomy_returns_the_embedded_v3_document_without_database() {
    use http_body_util::BodyExt;

    let response = app()
        .oneshot(Request::get("/api/taxonomy").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let actual: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../tools/nlp/topics-v3.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual["version"], "v3");
    assert!(!actual["topics"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn bad_pagination_is_rejected_without_database() {
    for query in [
        "page=0",
        "page_size=0",
        "page_size=101",
        "page=1000001",
        "page=abc",
        "page=-1",
    ] {
        let response = app()
            .oneshot(
                Request::get(format!("/api/documents?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn crawl_validates_input_without_authorization() {
    let response = app()
        .oneshot(
            Request::post("/api/crawls")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"source":"rfc","urls":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn crawl_requires_contact_without_authorization() {
    use http_body_util::BodyExt;
    let response = app()
        .oneshot(
            Request::post("/api/crawls")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"source":"rfc"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"].as_str().unwrap().contains("User-Agent"));
}

#[tokio::test]
async fn openapi_and_unknown_api_route() {
    use http_body_util::BodyExt;
    let response = app()
        .oneshot(
            Request::get("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(spec["paths"]["/api/taxonomy"]["get"].is_object());
    assert!(spec["paths"]["/api/documents"]["get"].is_object());
    assert!(spec["paths"]["/api/sources/export"]["get"].is_object());
    assert_eq!(
        spec["components"]["schemas"]["ExportFormat"]["enum"],
        serde_json::json!(["json", "jsonl", "csv", "zip"])
    );
    assert_local_refs_resolve(&spec, &spec);
    assert!(spec["components"]["securitySchemes"].is_null());
    for path in ["/api/crawls", "/api/sources/{source}/resume"] {
        assert!(spec["paths"][path]["post"]["security"].is_null());
        assert!(spec["paths"][path]["post"]["responses"]["401"].is_null());
    }
    let response = app()
        .oneshot(Request::get("/api/not-real").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

fn assert_local_refs_resolve(value: &serde_json::Value, document: &serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str)
                && let Some(pointer) = reference.strip_prefix('#')
            {
                assert!(
                    document.pointer(pointer).is_some(),
                    "unresolved OpenAPI reference: {reference}"
                );
            }
            for child in object.values() {
                assert_local_refs_resolve(child, document);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                assert_local_refs_resolve(child, document);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn invalid_export_parameters_are_rejected_before_database() {
    for query in [
        "format=xml",
        "source=github",
        "complete_only=yes",
        "page=1",
        "spreadsheet_safe=maybe",
    ] {
        let response = app()
            .oneshot(
                Request::get(format!("/api/sources/export?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn experiment_config_and_assignment_need_no_database() {
    let response = app()
        .oneshot(
            Request::get("/api/experiment?session=abcdef12-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["config"]["id"], "hybrid-vs-b0");
    assert!(["b0", "hybrid"].contains(&value["assignment"]["variant"].as_str().unwrap()));
}

#[tokio::test]
async fn a_malformed_session_is_rejected_before_any_work() {
    for uri in [
        "/api/experiment?session=short",
        "/api/search?q=x&session=has%20space%20in%20it",
        "/api/search?q=x&session=short",
    ] {
        let response = app()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
}
