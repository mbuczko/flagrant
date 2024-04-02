use thiserror::Error;

// Make our own error that wraps `anyhow::Error`.
// pub struct DbError(anyhow::Error);

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQL query failed")]
    QueryFailed,
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, DbError>`. That way we don't need to do that manually.
// impl<E> From<E> for DbError
// where
//     E: Into<anyhow::Error>,
// {
//     fn from(err: E) -> Self {
//         Self(err.into())
//     }
// }

