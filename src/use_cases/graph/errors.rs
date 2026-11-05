#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("document must look like rfc:3058 or internals:16678")]
    InvalidDocument,
    #[error("limit must be 1..200")]
    InvalidLimit,
    #[error("no such document")]
    NotFound,
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Repository(error.into())
    }
}
