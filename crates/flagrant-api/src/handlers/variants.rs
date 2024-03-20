use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{NewVariantRequestPayload, Variant};
use sqlx::SqlitePool;

use crate::errors::ServiceError;

/// Creates a new feature variant.
/// To create a new feature variant, a few pre-conditions must be fulfiled:
/// - there is still enough weight - all variants' weights should sum up to 100%
/// - variant should be created for all environments (by default with same value)
pub async fn create(
    State(pool): State<SqlitePool>,
    Path((project_id, feature_name, env_name)): Path<(u16, String, String)>,
    Json(variant): Json<NewVariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let feature = feature::fetch_by_name(&pool, &project, feature_name).await?;
    let env = environment::fetch_by_name(&pool, &project, env_name).await?;

    Ok(Json(
        variant::create(&pool, &env, &feature, variant.value, variant.weight).await?,
    ))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Path((project_id, feature_name, env_name)): Path<(u16, String, String)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let feature = feature::fetch_by_name(&pool, &project, feature_name).await?;
    let env = environment::fetch_by_name(&pool, &project, env_name).await?;

    Ok(Json(variant::list(&pool, &env, &feature).await?))
}

pub async fn delete(
    State(pool): State<SqlitePool>,
    Path((_project_id, variant_id)): Path<(u16, u16)>,
) -> Result<Json<()>, ServiceError> {
    Ok(Json(variant::delete(&pool, variant_id).await?))
}
