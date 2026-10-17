#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] pub sqlx::Error);
