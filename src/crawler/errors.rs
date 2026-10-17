#[derive(Debug, thiserror::Error)]
#[error("source access stopped: {0}")]
pub struct Blocked(pub String);
