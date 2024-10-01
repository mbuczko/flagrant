use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};
use flagrant::errors::FlagrantError;

use crate::errors::ServiceError;

pub struct Identity(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for Identity
where
    S: Send + Sync + 'static,
{
    type Rejection = ServiceError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(Ok(header)) = parts.headers.get("X-Flagrant-Identity").map(|h| h.to_str()) {
            return Ok(Identity(header.to_owned()) );
        }
        Err(FlagrantError::NoIdentity("No identity header found").into())
    }
}
