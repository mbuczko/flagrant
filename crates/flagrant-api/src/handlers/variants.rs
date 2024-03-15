use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{NewVariantRequestPayload, Variant};
use sqlx::SqlitePool;

use crate::errors::ServiceError;

pub async fn create(
    State(pool): State<SqlitePool>,
    Path((project_id, feature_name, env_name)): Path<(u16, String, String) >,
    Json(variant): Json<NewVariantRequestPayload>,
) -> Result<Json<Variant>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let feature = feature::fetch_by_name(&pool, &project, feature_name).await?;
    let env = environment::fetch_by_name(&pool, &project, env_name).await?;

    Ok(Json(variant::create(&pool, &env, &feature, variant.value, variant.weight).await?))
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
