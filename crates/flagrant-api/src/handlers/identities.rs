use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::{
    errors::FlagrantError,
    models::{environment, identity, project},
};
use flagrant_types::{
    IdentityWithTraits,
    payload::{IdentityPatch, NewIdentityPayload, OverridePayload},
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
    path = "/projects/{project}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("identity" = String, Path, description = "Identity value")
    ),
    responses(
        (status = 200, description = "Identity with traits", body = IdentityWithTraits),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path((project_name, identity_value)): Path<(String, String)>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identity = identity::get_by_value_with_traits(&mut conn, &project, identity_value).await?;

    Ok(Json(identity))
}

/// Creates a new identity with optional traits. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    post,
    path = "/projects/{project}/identities",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    request_body = NewIdentityPayload,
    responses(
        (status = 200, description = "Created identity with traits", body = IdentityWithTraits)
    ),
    tag = "identities"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Json(payload): Json<NewIdentityPayload>,
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

/// Applies a patch to an identity: optionally renames it and applies granular trait operations.
#[utoipa::path(
    patch,
    path = "/projects/{project}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("identity" = String, Path, description = "Identity value")
    ),
    request_body = IdentityPatch,
    responses(
        (status = 200, description = "Updated identity with traits", body = IdentityWithTraits),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((project_name, identity_value)): Path<(String, String)>,
    Json(patch): Json<IdentityPatch>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value).await?;
    let identity = identity::patch(&mut conn, &project, identity, patch).await?;

    Ok(Json(identity))
}

/// Returns the variant currently pinned to an identity for a given feature+environment,
/// or 404 if no override exists.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/identities/{identity}/override",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID"),
        ("identity" = String, Path, description = "Identity value")
    ),
    responses(
        (status = 200, description = "Current override", body = OverridePayload),
        (status = 404, description = "No override set for this identity")
    ),
    tag = "identities"
)]
pub async fn get_override(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, identity_value)): Path<(String, String, i32, String)>,
) -> Result<Json<OverridePayload>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value)
        .await
        .map_err(|_| FlagrantError::NotFound("No override set for this identity"))?;
    let variant_id = identity::get_variant_for_identity(&mut conn, &env, feature_id, &identity)
        .await?
        .ok_or(FlagrantError::NotFound("No override set for this identity"))?;
    Ok(Json(OverridePayload { variant_id }))
}

/// Overrides the variant assigned to an identity for a specific feature and environment,
/// bypassing normal variant distribution.
#[utoipa::path(
    put,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/identities/{identity}/override",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID"),
        ("identity" = String, Path, description = "Identity value")
    ),
    request_body = OverridePayload,
    responses(
        (status = 200, description = "Override applied"),
        (status = 404, description = "Identity or variant not found")
    ),
    tag = "identities"
)]
pub async fn set_override(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, identity_value)): Path<(String, String, i32, String)>,
    Json(payload): Json<OverridePayload>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value).await?;
    identity::override_variant(&mut conn, &env, &identity, feature_id, payload.variant_id).await?;
    Ok(Json(()))
}

/// Deletes an identity and all its trait associations and variant assignments.
#[utoipa::path(
    delete,
    path = "/projects/{project}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("identity" = String, Path, description = "Identity value")
    ),
    responses(
        (status = 200, description = "Identity deleted"),
        (status = 404, description = "Identity not found")
    ),
    tag = "identities"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_name, identity_value)): Path<(String, String)>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value).await?;

    identity::delete(&mut conn, identity).await?;
    Ok(Json(()))
}
