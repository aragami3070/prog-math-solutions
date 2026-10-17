#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] pub crate::repository::errors::Error);
