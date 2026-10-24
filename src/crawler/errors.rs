#[derive(Debug, thiserror::Error)]
#[error("source access stopped: {0}")]
pub struct Blocked(pub String);

/// A transient failure that persisted through every retry. Unlike `Blocked`, the source
/// did not refuse us; it just could not be reached or answered with a server error.
#[derive(Debug, thiserror::Error)]
#[error("source unreachable: {0}")]
pub struct Unavailable(pub String);
