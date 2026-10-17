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
    };
    router(AppState::new(pool, &config), &config.web_dist)
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
    assert!(spec["paths"]["/api/documents"]["get"].is_object());
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
