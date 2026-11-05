#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("query_id must be the id from a search response")]
    InvalidQueryId,
    #[error("document_id must look like rfc:3058 or internals:16678")]
    InvalidDocument,
    #[error("rank must be 1..1000")]
    InvalidRank,
    #[error("no such search")]
    UnknownSearch,
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Repository(error.into())
    }
}
