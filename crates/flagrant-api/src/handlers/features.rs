use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{feature, project};
use flagrant_types::{Feature, NewFeatureRequestPayload};
use sqlx::SqlitePool;

use crate::errors::ServiceError;

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
    Path((project_id, feature_name)): Path<(u16, String)>,
) -> Result<Json<Feature>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    Ok(Json(
        feature::fetch_by_name(&pool, &project, feature_name).await?,
    ))
}

pub async fn update(
    State(pool): State<SqlitePool>,
    Path((project_id, feature_name)): Path<(u16, String)>,
    Json(feat): Json<NewFeatureRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    feature::update_by_name(
        &pool,
        &project,
        feature_name,
        feat.name,
        feat.value,
        feat.is_enabled,
    )
    .await?;
    Ok(Json(()))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    Ok(Json(feature::list(&pool, &project).await?))
}
