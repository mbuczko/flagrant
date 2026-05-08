use std::str::FromStr;

use axum::{Json, extract::Path};
use flagrant::models::{environment, feature, variant};
use flagrant_types::{FeatureValue, Variant, payload::VariantRequestPayload};

use crate::{errors::ServiceError, extractors::DbConnection};

/// Creates a new feature variant.
///
/// A few pre-conditions must be met:
/// - there is enough free weight to create a variant with the given weight
/// - the variant should be created for all environments (with the same value by default)
#[utoipa::path(
    post,
    path = "/envs/{environment_id}/features/{feature_id}/variants",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
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
    Path((environment_id, feature_id)): Path<(i32, i32)>,
    Json(payload): Json<VariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let value = FeatureValue::from_str(&payload.value)?;
    let variant = variant::create(&mut conn, &env, &feature, value, payload.weight).await?;

    Ok(Json(variant))
}

/// Updates existing variant with provided value/weight.
#[utoipa::path(
    put,
    path = "/envs/{environment_id}/variants/{variant_id}",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
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
    Path((environment_id, variant_id)): Path<(i32, i32)>,
    Json(payload): Json<VariantRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;
    let value = FeatureValue::from_str(&payload.value)?;

    variant::update_one(&mut conn, &env, &var, value, payload.weight).await?;
    Ok(Json(()))
}

/// Fetches a variant by ID.
#[utoipa::path(
    get,
    path = "/envs/{environment_id}/variants/{variant_id}",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    responses(
        (status = 200, description = "Variant details", body = Variant)
    ),
    tag = "variants"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, variant_id)): Path<(i32, i32)>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let variant = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant))
}

/// Lists all variants for a feature.
#[utoipa::path(
    get,
    path = "/envs/{environment_id}/features/{feature_id}/variants",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    responses(
        (status = 200, description = "List of feature variants", body = Vec<Variant>)
    ),
    tag = "variants"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let variants = variant::get_for_feature(&mut conn, &env, feature.id).await?;

    Ok(Json(variants))
}

/// Deletes a variant.
#[utoipa::path(
    delete,
    path = "/envs/{environment_id}/variants/{variant_id}",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
        ("variant_id" = i32, Path, description = "Variant ID")
    ),
    responses(
        (status = 200, description = "Variant deleted successfully")
    ),
    tag = "variants"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, variant_id)): Path<(i32, i32)>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant::delete(&mut conn, &env, &var).await?))
}
