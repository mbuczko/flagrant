use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{Feature, NewFeatureRequestPayload, Variant};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    name: Option<String>,
}

pub async fn create(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
    Json(feat): Json<NewFeatureRequestPayload>,
) -> Result<Json<Feature>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let feature = feature::create(&pool, &project, feat.name, feat.value, feat.is_enabled).await?;

    Ok(Json(feature))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Path((project_id, feature_id, env_name)): Path<(u16, u16, String)>,
) -> Result<Json<Vec<Variant>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let env = environment::fetch_by_name(&pool, &project, env_name).await?;
    let feature = feature::fetch(&pool, feature_id).await?;

    Ok(Json(variant::list(&pool, &env, &feature).await?))
}
