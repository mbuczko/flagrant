use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, feature};
use flagrant_types::{Feature, payload::FeatureRequestPayload};
use serde::Deserialize;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize)]
pub(crate) struct FeatureQueryParams {
    prefix: Option<String>,
}

pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(environment_id): Path<i32>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::create(
        &mut conn,
        &env,
        payload.name,
        payload.value,
        payload.is_enabled,
    )
    .await?;

    Ok(Json(feature))
}

pub async fn fetch_by_id(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    Ok(Json(feature))
}

pub async fn fetch_by_name(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_name)): Path<(i32, String)>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_name(&mut conn, &env, feature_name).await?;

    Ok(Json(feature))
}

pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::update_one(&mut conn, &env, &feature)
        .name(payload.name)
        .value(payload.value)
        .enabled(payload.is_enabled)
        .update()
        .await?;

    Ok(Json(()))
}

pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<FeatureQueryParams>,
    Path(environment_id): Path<i32>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let features = match params.prefix {
        Some(prefix) => feature::get_by_prefix(&mut conn, &env, prefix).await?,
        _ => feature::get_all(&mut conn, &env).await?,
    };

    Ok(Json(features))
}

pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::delete(&mut conn, &env, &feature).await?;

    Ok(Json(()))
}
