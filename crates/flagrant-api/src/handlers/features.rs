use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{feature, project};
use flagrant_types::{Feature, NewFeatureRequestPayload};
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

pub async fn fetch(
    State(pool): State<SqlitePool>,
    Path((_project_id, feature_id)): Path<(u16, u16)>,
) -> Result<Json<Feature>, ServiceError> {
    Ok(Json(feature::fetch(&pool, feature_id).await?))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    Ok(Json(feature::list(&pool, &project).await?))
}
