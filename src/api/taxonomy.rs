use super::AppState;
use axum::{Json, Router, routing::get};
use serde_json::Value;

pub fn router() -> Router<AppState> {
    Router::new().route("/taxonomy", get(get_taxonomy))
}

#[utoipa::path(
    get,
    path = "/api/taxonomy",
    responses((status = 200, description = "Versioned topic taxonomy", body = Object))
)]
pub async fn get_taxonomy() -> Json<Value> {
    Json(
        serde_json::from_str(include_str!("../../tools/nlp/topics-v3.json"))
            .expect("embedded v3 taxonomy must be valid"),
    )
}
