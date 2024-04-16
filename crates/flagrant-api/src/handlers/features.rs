use axum::{
    extract::{Path, Query, State},
    Json,
};
use flagrant::models::{environment, feature};
use flagrant_types::{Feature, FeatureRequestPayload};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

#[derive(Debug, Deserialize)]
pub struct FeatureQueryParams {
    name: Option<String>,
}

pub async fn create(
    State(pool): State<SqlitePool>,
    Path(environment_id): Path<u16>,
    Json(feat): Json<FeatureRequestPayload>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::create(&pool, &env, feat.name, feat.value, feat.is_enabled).await?;

    Ok(Json(feature))
}

pub async fn fetch(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;

    Ok(Json(feature))
}

pub async fn update(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
    Json(feat): Json<FeatureRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;
    feature::update(
        &pool,
        &env,
        &feature,
        feat.name,
        feat.value,
        feat.is_enabled,
    )
    .await?;

    Ok(Json(()))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Query(params): Query<FeatureQueryParams>,
    Path(environment_id): Path<u16>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let features = match params.name {
        Some(name) => vec![feature::fetch_by_name(&pool, &env, name).await?],
        _ => feature::list(&pool, &env).await?
    };

    Ok(Json(features))
}

pub async fn delete(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_id)): Path<(u16, u16)>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch(&pool, &env, feature_id).await?;
    feature::delete(
        &pool,
        &env,
        &feature,
    )
    .await?;

    Ok(Json(()))
}
