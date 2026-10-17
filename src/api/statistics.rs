use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{models::statistics::Statistics, use_cases::statistics};
use axum::{Json, Router, extract::State, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/stats", get(get_statistics))
}

#[utoipa::path(
    get,
    path = "/api/stats",
    responses(
        (status = 200, body = Statistics),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn get_statistics(State(state): State<AppState>) -> Result<Json<Statistics>, Error> {
    let mut tx = state
        .pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let result = statistics::get::execute(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
