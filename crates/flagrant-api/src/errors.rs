use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use flagrant::errors::FlagrantError;

// Make our own error that wraps `anyhow::Error`.
pub struct ServiceError(anyhow::Error);

impl IntoResponse for ServiceError {
    /// Never responds with a 5xx: whatever went wrong (a failed query, an unexpected
    /// internal error, ...) is logged here with full detail so it's not lost, but the
    /// caller only ever sees a 4xx - there's no client action that fixes a genuine server
    /// fault, so surfacing 500 buys nothing beyond what the logs already capture, and a
    /// consistent 4xx keeps client-side error handling simple.
    fn into_response(self) -> Response<Body> {
        match self.0.downcast_ref::<FlagrantError>() {
            Some(FlagrantError::UnexpectedFailure(error, cause)) => {
                tracing::error!(cause = ?cause, error);
                (StatusCode::BAD_REQUEST, error.to_string())
            }
            Some(FlagrantError::QueryFailed(error, cause)) => {
                tracing::error!(cause = ?cause, error);
                (StatusCode::BAD_REQUEST, error.to_string())
            }
            Some(FlagrantError::BadRequest(error)) => {
                tracing::error!(error);
                (StatusCode::BAD_REQUEST, error.to_string())
            }
            Some(FlagrantError::NoIdentity(error)) => {
                tracing::error!(error);
                (StatusCode::UNAUTHORIZED, error.to_string())
            }
            Some(FlagrantError::NotFound(error)) => (StatusCode::NOT_FOUND, error.to_string()),
            _ => {
                tracing::error!(error = ?self.0, "Unexpected error");
                (StatusCode::BAD_REQUEST, format!("Error: {}", self.0))
            }
        }
        .into_response()
    }
}

/// Enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
/// `Result<_, ServiceError>`, so the conversion does not need to be done manually.
impl<E> From<E> for ServiceError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
