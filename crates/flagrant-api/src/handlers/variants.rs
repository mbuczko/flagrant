use axum::{Json, extract::Path};
use flagrant::models::{environment, feature, identity, project, variant};
use flagrant_types::Variant;

use crate::{errors::ServiceError, extractors::DbConnection};

/// Fetches a variant by ID.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/variants/{variant_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    responses(
        (status = 200, description = "Variant details", body = Variant)
    ),
    tag = "variants"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, variant_id)): Path<(String, String, i32)>,
) -> Result<Json<Variant>, ServiceError> {
    let proj = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let variant = variant::get_by_id(&mut conn, &env, variant_id, None).await?;

    Ok(Json(variant))
}

/// Lists all variants for a feature.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/variants",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    responses(
        (status = 200, description = "List of feature variants", body = Vec<Variant>)
    ),
    tag = "variants"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id)): Path<(String, String, i32)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let proj = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let variants = variant::get_for_feature(&mut conn, &env, feature.id, None).await?;

    Ok(Json(variants))
}

/// Lists identity values explicitly pinned to a variant.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/variants/{variant_id}/identities",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    responses(
        (status = 200, description = "Identities pinned to this variant", body = Vec<String>)
    ),
    tag = "variants"
)]
pub async fn get_pinned_identities(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, variant_id)): Path<(String, String, i32)>,
) -> Result<Json<Vec<String>>, ServiceError> {
    let proj = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id, None).await?;
    let identities = identity::list_identities_pinned_to_variant(&mut conn, env.id, var.id).await?;

    Ok(Json(identities))
}
