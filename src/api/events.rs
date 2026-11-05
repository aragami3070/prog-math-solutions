use super::{
    AppState,
    errors::{Error, ErrorBody},
};
use crate::{
    experiment::{Config, EXPERIMENT, valid_session},
    models::events::{ClickEvent, ExperimentInfo},
    use_cases::events,
};
use axum::{
    Json, Router,
    extract::{Query, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events/click", post(click))
        .route("/experiment", get(experiment))
}

/// A click on a search result, tied to the search by the `query_id` that search returned.
/// The session and the arm come from the recorded search, not from the client.
#[utoipa::path(
    post,
    path = "/api/events/click",
    request_body = ClickEvent,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
)]
pub async fn click(
    State(state): State<AppState>,
    body: Result<Json<ClickEvent>, JsonRejection>,
) -> Result<StatusCode, Error> {
    let Json(click) =
        body.map_err(|_| Error(StatusCode::BAD_REQUEST, "Invalid click event".into()))?;
    let mut tx = state.pool.begin().await?;
    events::click::execute(&mut tx, click).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ExperimentQuery {
    /// Also say which arm this session is in.
    pub session: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ExperimentView {
    pub config: &'static Config,
    /// The arm of `session`, when one was given.
    pub assignment: Option<ExperimentInfo>,
}

/// The pre-registered experiment: arms, shares, primary metric, smallest effect of interest.
#[utoipa::path(
    get,
    path = "/api/experiment",
    params(ExperimentQuery),
    responses((status = 200, body = ExperimentView), (status = 400, body = ErrorBody)),
)]
pub async fn experiment(
    query: Result<Query<ExperimentQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<ExperimentView>, Error> {
    let Query(query) =
        query.map_err(|_| Error(StatusCode::BAD_REQUEST, "Invalid parameters".into()))?;
    let assignment = match query.session.as_deref() {
        None => None,
        Some(session) if valid_session(session) => Some(ExperimentInfo {
            id: EXPERIMENT.id,
            version: EXPERIMENT.version,
            variant: EXPERIMENT.assign(session).name,
            query_id: None,
        }),
        Some(_) => return Err(Error(StatusCode::BAD_REQUEST, "Invalid session".into())),
    };
    Ok(Json(ExperimentView {
        config: &EXPERIMENT,
        assignment,
    }))
}
