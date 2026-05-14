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
}
