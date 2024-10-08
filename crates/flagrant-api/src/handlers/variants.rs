use std::str::FromStr;

use axum::{extract::Path, Json};
use flagrant::models::{environment, feature, variant};
use flagrant_types::{payload::VariantRequestPayload, FeatureValue, Variant};

use crate::{errors::ServiceError, extractors::DbConnection};

/// Creates a new feature variant.
/// To create a new feature variant, a few pre-conditions must be fulfiled:
/// - there is still enough weight - all variants' weights should sum up to 100%
/// - variant should be created for all environments (by default with same value)
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
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, variant_id)): Path<(i32, i32)>,
    Json(payload): Json<VariantRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;
    let value = FeatureValue::from_str(&payload.value)?;

    variant::update(&mut conn, &env, &var, value, payload.weight).await?;
    Ok(Json(()))
}

pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, variant_id)): Path<(i32, i32)>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let variant = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant))
}

pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;
    let variants = variant::get_all(&mut conn, &env, feature.id).await?;

    Ok(Json(variants))
}

pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, variant_id)): Path<(i32, i32)>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let var = variant::get_by_id(&mut conn, &env, variant_id).await?;

    Ok(Json(variant::delete(&mut conn, &env, &var).await?))
}
