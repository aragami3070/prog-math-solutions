#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("page must be 1..1000000 and page_size 1..100")]
    InvalidPagination,
    #[error("could not generate corpus export")]
    Export(#[from] std::io::Error),
    #[error("could not serialize corpus export")]
    Json(#[from] serde_json::Error),
    #[error("could not serialize CSV export")]
    Csv(#[from] csv::Error),
    #[error("could not build export archive")]
    Zip(#[from] zip::result::ZipError),
    #[error("export task failed")]
    Task(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Repository(#[from] crate::repository::errors::Error),
}
