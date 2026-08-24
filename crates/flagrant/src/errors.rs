use thiserror::Error;

#[derive(Error, Debug)]
pub enum FlagrantError {
    #[error("Bad request ({0})")]
    BadRequest(&'static str),

    #[error("Unexpected failure ({0})")]
    UnexpectedFailure(&'static str, anyhow::Error),

    #[error("Query failed ({0}). Cause: {1}")]
    QueryFailed(&'static str, sqlx::Error),

    #[error("Request containst no identity ({0})")]
    NoIdentity(&'static str),

    #[error("Invalid operation: {0}")]
    InvalidOperation(&'static str),

    #[error("Not found: {0}")]
    NotFound(&'static str),

    /// `kind` is "feature" or "segment". Raised in `commit::apply`, the only place
    /// this is checked, right after fetching the entity and before applying the patch -
    /// covers both modification and deletion, since delete is just a branch of patch.
    #[error(
        "Version conflict on {kind}: you were working from version {expected}, but the current version is {current} - it was modified elsewhere since you last fetched it. Refresh and reapply your change."
    )]
    VersionMismatch {
        kind: &'static str,
        id: i32,
        expected: i32,
        current: i32,
    },
}
