use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use thiserror::Error;

// Make our own error that wraps `anyhow::Error`.
pub struct ServiceError(anyhow::Error);

#[derive(Error, Debug)]
pub enum InternalError {
    // #[error("Malformed cookie")]
    // MalformedCookie,
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response<Body> {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}

/// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
/// `Result<_, ServiceError>`. That way we don't need to do that manually.
impl<E> From<E> for ServiceError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
