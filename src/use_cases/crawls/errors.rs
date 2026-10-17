#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("another crawl is already running")]
    Busy,
    #[error("source is paused: {0}")]
    Paused(String),
    #[error("set a crawler User-Agent with a real contact before collecting")]
    Disabled,
    #[error("invalid crawl request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}
