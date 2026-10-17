use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::use_cases::health;
use axum::{Router, extract::State, http::StatusCode, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(check))
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 204, description = "Database ready"),
        (status = 503, body = ErrorBody),
    ),
)]
pub async fn check(State(state): State<AppState>) -> Result<StatusCode, Error> {
    let mut tx = state.pool.begin().await?;
    health::check::execute(&mut tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
