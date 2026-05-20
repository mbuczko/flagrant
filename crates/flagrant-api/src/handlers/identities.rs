use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{identity, project};
use flagrant_types::{
    IdentityWithTraits,
    payload::{IdentityRequestPayload, IdentityTraitPayload},
};
use serde::Deserialize;
use utoipa::IntoParams;


#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct IdentityQueryParams {
    /// Filter by identity prefix
    prefix: Option<String>,
    /// Optional pattern to filter identities (substring match, max 10 returned)
    pattern: Option<String>,
}

/// Lists up to 10 identities with their traits, optionally filtered by a pattern.
#[utoipa::path(
    get,
    path = "/projects/{project}/identities",
    params(
        ("project" = String, Path, description = "Project name"),
        IdentityQueryParams
    ),
    responses(
        (status = 200, description = "List of identities with traits", body = Vec<IdentityWithTraits>)
    ),
    tag = "identities"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Query(params): Query<IdentityQueryParams>,
) -> Result<Json<Vec<IdentityWithTraits>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identities = identity::list(
        &mut conn,
        &project,
        super::parse_pattern(params.pattern, params.prefix),
    )
    .await?;
    Ok(Json(identities))
}

/// Fetches a single identity with its traits.
#[utoipa::path(
    get,
    path = "/projects/{project}/identities/{identity_id}",
    params(
        ("project" = String, Path, description = "Project name"),
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
    Path((_project, identity_id)): Path<(String, i32)>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let identity = identity::get_with_traits_by_id(&mut conn, identity_id).await?;
    Ok(Json(identity))
}

/// Creates a new identity with optional traits. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    post,
    path = "/projects/{project}/identities",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    request_body = IdentityRequestPayload,
    responses(
        (status = 200, description = "Created identity with traits", body = IdentityWithTraits)
    ),
    tag = "identities"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Json(payload): Json<IdentityRequestPayload>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identity = identity::create(
        &mut conn,
        &project,
        payload.identity,
        payload.traits.unwrap_or_default(),
    )
    .await?;
    Ok(Json(identity))
}

/// Replaces all traits for an identity. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    put,
    path = "/projects/{project}/identities/{identity_id}",
    params(
        ("project" = String, Path, description = "Project name"),
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
    Path((project_name, identity_id)): Path<(String, i32)>,
    Json(traits): Json<Vec<IdentityTraitPayload>>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identity = identity::get_by_id(&mut conn, identity_id).await?;
    let identity = identity::update_traits(&mut conn, &project, identity, traits).await?;

    Ok(Json(identity))
}

/// Deletes an identity and all its trait associations and variant assignments.
#[utoipa::path(
    delete,
    path = "/projects/{project}/identities/{identity_id}",
    params(
        ("project" = String, Path, description = "Project name"),
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
    Path((_project, identity_id)): Path<(String, i32)>,
) -> Result<Json<()>, ServiceError> {
    let identity = identity::get_by_id(&mut conn, identity_id).await?;
    identity::delete(&mut conn, identity).await?;

    Ok(Json(()))
}
