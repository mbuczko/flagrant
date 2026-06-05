use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, identity, project};
use flagrant_types::{
    IdentityVariant, IdentityWithTraits,
    payload::{IdentityPatch, NewIdentityPayload},
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

/// Applies a patch to an identity: applies granular trait operations
/// and pins the identity to specific variants per feature (overrides).
#[utoipa::path(
    patch,
    path = "/projects/{project}/envs/{environment}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
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
    Path((project_name, env_name, identity_value)): Path<(String, String, String)>,
    Json(patch): Json<IdentityPatch>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value).await?;
    let identity = identity::patch(&mut conn, &project, &env, identity, patch).await?;

    Ok(Json(identity))
}


/// Returns all variant assignments for an identity within a given environment.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/identities/{identity}/variants",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("identity" = String, Path, description = "Identity value")
    ),
    responses(
        (status = 200, description = "Variant assignments for this identity", body = Vec<IdentityVariant>)
    ),
    tag = "identities"
)]
pub async fn get_variants(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, identity_value)): Path<(String, String, String)>,
) -> Result<Json<Vec<IdentityVariant>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value(&mut conn, &project, identity_value).await?;
    let variants = identity::list_variant_assignments(&mut conn, &env, &identity).await?;
    Ok(Json(variants))
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
