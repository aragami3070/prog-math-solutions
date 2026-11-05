#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("page must be 1..1000000 and page_size 1..100")]
    InvalidPagination,
    #[error("{0}")]
    InvalidQuery(&'static str),
    #[error("search index is not built; POST /api/search/rebuild first")]
    NotBuilt,
    #[error("the index was rebuilt since the first page; repeat the search from page 1")]
    GenerationChanged,
    #[error("the index is being rebuilt; try again later")]
    Busy,
    #[error("search index failed")]
    Index(#[source] crate::search::Error),
    #[error("search task failed")]
    Task(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}

impl From<crate::search::Error> for Error {
    fn from(error: crate::search::Error) -> Self {
        match error {
            crate::search::Error::InvalidQuery(reason) => Self::InvalidQuery(reason),
            error => Self::Index(error),
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Repository(error.into())
    }
}
