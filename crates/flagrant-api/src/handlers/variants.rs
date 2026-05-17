use std::str::FromStr;

use axum::{Json, extract::Path};
use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{FeatureValue, Variant, payload::VariantRequestPayload};

use crate::{errors::ServiceError, extractors::DbConnection};

/// Creates a new feature variant.
///
/// A few pre-conditions must be met:
/// - there is enough free weight to create a variant with the given weight
/// - the variant should be created for all environments (with the same value by default)
#[utoipa::path(
    post,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}/variants",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    request_body = VariantRequestPayload,
    responses(
        (status = 200, description = "Created variant", body = Variant)
    ),
    tag = "variants"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, feature_id)): Path<(i32, String, i32)>,
    Json(payload): Json<VariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let value = FeatureValue::from_str(&payload.value)?;
    let variant = variant::create(&mut conn, &env, &feature, value, payload.weight).await?;

    Ok(Json(variant))
}

/// Updates existing variant with provided value/weight.
#[utoipa::path(
    put,
    path = "/projects/{project_id}/envs/{environment}/variants/{variant_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    request_body = VariantRequestPayload,
    responses(
        (status = 200, description = "Variant updated successfully")
    ),
    tag = "variants"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, variant_id)): Path<(i32, String, i32)>,
    Json(payload): Json<VariantRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;
    let value = FeatureValue::from_str(&payload.value)?;

    variant::update_one(&mut conn, &env, &var, value, payload.weight).await?;
    Ok(Json(()))
}

/// Fetches a variant by ID.
#[utoipa::path(
    get,
    path = "/projects/{project_id}/envs/{environment}/variants/{variant_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
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
    Path((project_id, env_name, variant_id)): Path<(i32, String, i32)>,
) -> Result<Json<Variant>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let variant = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant))
}

/// Lists all variants for a feature.
#[utoipa::path(
    get,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}/variants",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
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
    Path((project_id, env_name, feature_id)): Path<(i32, String, i32)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let variants = variant::get_for_feature(&mut conn, &env, feature.id).await?;

    Ok(Json(variants))
}

/// Deletes a variant.
#[utoipa::path(
    delete,
    path = "/projects/{project_id}/envs/{environment}/variants/{variant_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    responses(
        (status = 200, description = "Variant deleted successfully")
    ),
    tag = "variants"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, variant_id)): Path<(i32, String, i32)>,
) -> Result<Json<()>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant::delete(&mut conn, &env, &var).await?))
}
