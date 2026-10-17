#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("page must be 1..1000000 and page_size 1..100")]
    InvalidPagination,
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}
