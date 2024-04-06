use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, feature, variant};
use flagrant_types::{NewVariantRequestPayload, Variant};
use sqlx::SqlitePool;

use crate::errors::ServiceError;

/// Creates a new feature variant.
/// To create a new feature variant, a few pre-conditions must be fulfiled:
/// - there is still enough weight - all variants' weights should sum up to 100%
/// - variant should be created for all environments (by default with same value)
pub async fn create(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
    Json(variant): Json<NewVariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;
    let variant = variant::create(&pool, &env, &feature, variant.value, variant.weight).await?;

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
    Path((_environment_id, _feature_id, variant_id)): Path<(u16, u16, u16)>,
) -> Result<Json<()>, ServiceError> {
    Ok(Json(variant::delete(&pool, variant_id).await?))
}
