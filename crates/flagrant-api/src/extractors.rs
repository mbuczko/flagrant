use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use flagrant::errors::FlagrantError;
use sqlx::{pool::PoolConnection, Sqlite, SqlitePool};

use crate::errors::ServiceError;

pub struct Identity(pub String);
pub struct DbConnection(pub PoolConnection<Sqlite>);

#[async_trait]
impl<S> FromRequestParts<S> for Identity
where
    S: Send + Sync,
{
    type Rejection = ServiceError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(Ok(header)) = parts.headers.get("X-Flagrant-Identity").map(|h| h.to_str()) {
            return Ok(Identity(header.to_owned()));
        }
        Err(FlagrantError::NoIdentity("No identity header found").into())
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for DbConnection
where
    SqlitePool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ServiceError;

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(DbConnection(SqlitePool::from_ref(state).acquire().await?))
    }
}
