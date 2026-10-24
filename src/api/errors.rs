use crate::use_cases;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

pub struct Error(pub StatusCode, pub String);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

impl Error {
    fn database(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "database request failed");
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database temporarily unavailable".into(),
        )
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::database(error)
    }
}
impl From<use_cases::sources::errors::Error> for Error {
    fn from(error: use_cases::sources::errors::Error) -> Self {
        match error {
            use_cases::sources::errors::Error::InvalidPagination => {
                Self(StatusCode::BAD_REQUEST, error.to_string())
            }
            use_cases::sources::errors::Error::Repository(error) => Self::database(error),
            error => {
                tracing::error!(%error, "corpus export failed");
                Self(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not generate corpus export".into(),
                )
            }
        }
    }
}
impl From<use_cases::crawls::errors::Error> for Error {
    fn from(error: use_cases::crawls::errors::Error) -> Self {
        use use_cases::crawls::errors::Error as CrawlError;
        match error {
            CrawlError::Invalid(_) => Self(StatusCode::BAD_REQUEST, error.to_string()),
            CrawlError::Disabled => Self(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
            CrawlError::Busy | CrawlError::Paused(_) => {
                Self(StatusCode::CONFLICT, error.to_string())
            }
            CrawlError::Sources(error) => Self::from(error),
            CrawlError::Repository(error) => Self::database(error),
        }
    }
}
impl From<use_cases::statistics::errors::Error> for Error {
    fn from(error: use_cases::statistics::errors::Error) -> Self {
        Self::database(error)
    }
}
impl From<use_cases::health::errors::Error> for Error {
    fn from(error: use_cases::health::errors::Error) -> Self {
        Self::database(error)
    }
}
impl From<use_cases::search::errors::Error> for Error {
    fn from(error: use_cases::search::errors::Error) -> Self {
        use use_cases::search::errors::Error as SearchError;
        match error {
            SearchError::InvalidPagination | SearchError::InvalidQuery(_) => {
                Self(StatusCode::BAD_REQUEST, error.to_string())
            }
            SearchError::NotBuilt => Self(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
            SearchError::Busy => Self(StatusCode::CONFLICT, error.to_string()),
            SearchError::Repository(error) => Self::database(error),
            error => {
                tracing::error!(%error, "search failed");
                Self(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Search is temporarily unavailable".into(),
                )
            }
        }
    }
}
