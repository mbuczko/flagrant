use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::identity::TraitCondition;
use flagrant::models::{environment, identity, project};
use flagrant_types::{
    IdentityVariant, IdentityWithTraits,
    payload::{IdentityPatch, NewIdentityPayload},
};
use serde::Deserialize;
use smallvec::{SmallVec, smallvec};
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct IdentityQueryParams {
    /// Filter by identity prefix
    prefix: Option<String>,
    /// Optional pattern to filter identities (substring match, max 10 returned)
    pattern: Option<String>,
    /// Comma-separated trait conditions to filter by: `name` (has trait, any value),
    /// `name=value` (has trait matching value - coerced to bool/int/float/string as
    /// applicable), or either prefixed with `-` to exclude (e.g. "vip,-churned,-country=us")
    traits: Option<String>,
}

type TraitConditionsTuple<'a> = (
    Option<SmallVec<[TraitCondition<'a>; 3]>>,
    Option<SmallVec<[TraitCondition<'a>; 3]>>,
);

/// Parses a `traits` query parameter into included/excluded [`TraitCondition`] lists.
/// Each comma-separated entry is `name`, `name=value`, `-name`, or `-name=value`; a
/// leading `-` marks the condition as excluded. `=value` pins the condition to a value
/// coercible from the raw string (bool/int/float/string); without it, any value matches -
/// for exclusions, this means "does not have this trait at all".
fn parse_trait_conditions(traits: Option<&String>) -> TraitConditionsTuple<'_> {
    let Some(traits) = traits else {
        return (None, None);
    };
    let (mut included, mut excluded): (SmallVec<[_; 3]>, SmallVec<[_; 3]>) =
        (smallvec![], smallvec![]);

    for entry in traits.split(',') {
        let (entry, excl) = match entry.strip_prefix('-') {
            Some(rest) => (rest, true),
            None => (entry, false),
        };
        if entry.is_empty() {
            continue;
        }

        let condition = match entry.split_once('=') {
            Some((name, value)) if !name.is_empty() && !value.is_empty() => {
                TraitCondition::value(name, value)
            }
            _ => TraitCondition::any_value(entry),
        };

        if excl {
            excluded.push(condition);
        } else {
            included.push(condition);
        }
    }

    (
        if included.is_empty() {
            None
        } else {
            Some(included)
        },
        if excluded.is_empty() {
            None
        } else {
            Some(excluded)
        },
    )
}

/// Lists up to 10 identities with their traits, optionally filtered by a pattern and/or trait.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/identities",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        IdentityQueryParams
    ),
    responses(
        (status = 200, description = "List of identities with traits", body = Vec<IdentityWithTraits>)
    ),
    tag = "identities"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name)): Path<(String, String)>,
    Query(params): Query<IdentityQueryParams>,
) -> Result<Json<Vec<IdentityWithTraits>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let (traits_included, traits_excluded) = parse_trait_conditions(params.traits.as_ref());
    let identities = identity::list(
        &mut conn,
        &env,
        super::parse_pattern(params.pattern, params.prefix),
        traits_included,
        traits_excluded,
    )
    .await?;

    Ok(Json(identities))
}

/// Fetches a single identity with its traits.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
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
    Path((project_name, env_name, identity_value)): Path<(String, String, String)>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value_with_traits(&mut conn, &env, identity_value).await?;

    Ok(Json(identity))
}

/// Creates a new identity with optional traits. Traits are auto-created if they don't exist yet.
#[utoipa::path(
    post,
    path = "/projects/{project}/envs/{environment}/identities",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name")
    ),
    request_body = NewIdentityPayload,
    responses(
        (status = 200, description = "Created identity with traits", body = IdentityWithTraits)
    ),
    tag = "identities"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name)): Path<(String, String)>,
    Json(payload): Json<NewIdentityPayload>,
) -> Result<Json<IdentityWithTraits>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::create(
        &mut conn,
        &env,
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
    let identity = identity::get_by_value(&mut conn, &env, identity_value).await?;
    let identity = identity::patch(&mut conn, &env, identity, patch).await?;

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
    let identity = identity::get_by_value(&mut conn, &env, identity_value).await?;
    let variants = identity::list_variant_assignments(&mut conn, &env, &identity).await?;
    Ok(Json(variants))
}

/// Deletes an identity and all its trait associations and variant assignments.
#[utoipa::path(
    delete,
    path = "/projects/{project}/envs/{environment}/identities/{identity}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
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
    Path((project_name, env_name, identity_value)): Path<(String, String, String)>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_by_value(&mut conn, &env, identity_value).await?;

    identity::delete(&mut conn, identity).await?;
    Ok(Json(()))
}

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct ClearIdentitiesParams {
    /// Pattern matching identities to delete. Use `*` as a wildcard (e.g. "user-*", or "*"
    /// to match every identity in the environment).
    pattern: String,
}

/// Deletes every identity (and its traits/variant assignments) in this environment whose
/// value matches `pattern`.
#[utoipa::path(
    delete,
    path = "/projects/{project}/envs/{environment}/identities",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ClearIdentitiesParams
    ),
    responses(
        (status = 200, description = "Matching identities deleted")
    ),
    tag = "identities"
)]
pub async fn clear(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name)): Path<(String, String)>,
    Query(params): Query<ClearIdentitiesParams>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let like_pattern = params.pattern.replace('*', "%");

    identity::clear_matching(&mut conn, &env, &like_pattern).await?;
    Ok(Json(()))
}
