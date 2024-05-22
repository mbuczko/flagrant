use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use flagrant::errors::FlagrantError;

// Make our own error that wraps `anyhow::Error`.
pub struct ServiceError(anyhow::Error);

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response<Body> {
        match self.0.downcast_ref::<FlagrantError>() {
            Some(FlagrantError::UnexpectedFailure(error, cause)) => {
                tracing::error!(cause = ?cause, error);
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            Some(FlagrantError::QueryFailed(error, cause)) => {
                tracing::error!(cause = ?cause, error);
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            Some(FlagrantError::BadRequest(error)) => {
                tracing::error!(error);
                (StatusCode::BAD_REQUEST, error.to_string())
            }
            _ => {
                tracing::error!(error = ?self.0, "Unexpected error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Error: {}", self.0),
                )
            }
        }
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
