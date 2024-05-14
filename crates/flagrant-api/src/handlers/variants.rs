use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, feature, variant};
use flagrant_types::{VariantRequestPayload, Variant};
use sqlx::SqlitePool;

use crate::errors::ServiceError;

/// Creates a new feature variant.
/// To create a new feature variant, a few pre-conditions must be fulfiled:
/// - there is still enough weight - all variants' weights should sum up to 100%
/// - variant should be created for all environments (by default with same value)
pub async fn create(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
    Json(variant): Json<VariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;
    let variant = variant::create(&pool, &env, &feature, variant.value, variant.weight).await?;

    Ok(Json(variant))
}

/// Updates existing variant with provided value/weight.
pub async fn update(
    State(pool): State<SqlitePool>,
    Path((environment_id, variant_id)): Path<(u16, u16)>,
    Json(variant): Json<VariantRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let var = variant::fetch(&pool, &env, variant_id).await?;

    variant::update(&pool, &env, &var, variant.value, variant.weight).await?;
    Ok(Json(()))
}

pub async fn fetch(
    State(pool): State<SqlitePool>,
    Path((environment_id, variant_id)): Path<(u16, u16)>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let variant = variant::fetch(&pool, &env, variant_id).await?;

    Ok(Json(variant))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;
    let variants = variant::list(&pool, &env, &feature).await?;

    Ok(Json(variants))
}

pub async fn delete(
    State(pool): State<SqlitePool>,
    Path((environment_id, variant_id)): Path<(u16, u16)>,
) -> Result<Json<()>, ServiceError> {
    let mut conn = pool.acquire().await?;
    let env = environment::fetch(&pool, environment_id).await?;

    Ok(Json(variant::delete(&mut conn, &env, variant_id).await?))
}
