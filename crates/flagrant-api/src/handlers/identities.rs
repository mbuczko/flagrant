use axum::{Json, extract::{Path, Query}};
use flagrant::models::identity;
use flagrant_types::{IdentityWithTraits, payload::{IdentityRequestPayload, IdentityTraitPayload}};
use serde::Deserialize;
use utoipa::IntoParams;
use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct IdentityQueryParams {
    /// Optional pattern to filter identities (substring match, max 10 returned)
    pattern: Option<String>,
}

/// Lists up to 10 identities with their traits, optionally filtered by a pattern.
#[utoipa::path(
    get,
    path = "/identities",
    params(IdentityQueryParams),
    responses(
        (status = 200, description = "List of identities with traits", body = Vec<IdentityWithTraits>)
    ),
    tag = "identities"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<IdentityQueryParams>,
) -> Result<Json<Vec<IdentityWithTraits>>, ServiceError> {
    let identities = identity::list(&mut conn, params.pattern).await?;
    Ok(Json(identities))
}

/// Fetches a single identity with its traits.
#[utoipa::path(
    get,
    path = "/identities/{identity_id}",
    params(
        ("identity_id" = i32, Path, description = "Identity ID")
    ),
    responses(
        (status = 200, description = "Identity with traits", body = IdentityWithTraits),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path(identity_id): Path<i32>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let identity = identity::get_by_id(&mut conn, identity_id).await?;
    Ok(Json(identity))
}

/// Creates a new identity with optional traits. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    post,
    path = "/identities",
    request_body = IdentityRequestPayload,
    responses(
        (status = 200, description = "Created identity with traits", body = IdentityWithTraits)
    ),
    tag = "identities"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Json(payload): Json<IdentityRequestPayload>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let identity = identity::create(
        &mut conn,
        payload.identity,
        payload.traits.unwrap_or_default(),
    ).await?;
    Ok(Json(identity))
}

/// Replaces all traits for an identity. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    put,
    path = "/identities/{identity_id}",
    params(
        ("identity_id" = i32, Path, description = "Identity ID")
    ),
    request_body = Vec<IdentityTraitPayload>,
    responses(
        (status = 200, description = "Updated identity with traits", body = IdentityWithTraits),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path(identity_id): Path<i32>,
    Json(traits): Json<Vec<IdentityTraitPayload>>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let identity = identity::update_traits(&mut conn, identity_id, traits).await?;
    Ok(Json(identity))
}

/// Deletes an identity and all its trait associations and variant assignments.
#[utoipa::path(
    delete,
    path = "/identities/{identity_id}",
    params(
        ("identity_id" = i32, Path, description = "Identity ID")
    ),
    responses(
        (status = 200, description = "Identity deleted"),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path(identity_id): Path<i32>,
) -> Result<Json<()>, ServiceError> {
    identity::delete(&mut conn, identity_id).await?;
    Ok(Json(()))
}
