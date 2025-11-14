use std::fmt::format;

use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, feature};
use flagrant_types::{Feature, payload::FeatureRequestPayload};
use serde::Deserialize;
use smallvec::smallvec;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize)]
pub(crate) struct FeatureQueryParams {
    prefix: Option<String>,
    status: Option<String>,
    state: Option<String>,
    tags: Option<String>,
    pattern: Option<String>,
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
        false,
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
        // TODO: get_by_prefix is unnecessary - reuse get_all with additional (prefix) parameter
        Some(prefix) => feature::get_by_prefix(&mut conn, &env, prefix).await?,
        _ => {
            let status = params
                .status
                .filter(|s| !s.is_empty())
                .map(|s| s == "active");
            let state = params.state.filter(|s| !s.is_empty()).map(|s| s == "on");
            let pattern = params
                .pattern
                .filter(|s| !s.is_empty())
                .map(|p| format!("%{p}%"));
            let (tags_included, tags_excluded) = params
                .tags
                .as_ref()
                .map(|tags| {
                    let (mut included, mut excluded) = (smallvec![], smallvec![]);
                    for tag in tags.split(',') {
                        if let Some(tag) = tag.strip_prefix('-')
                            && !tag.is_empty()
                        {
                            excluded.push(tag);
                        } else if !tag.is_empty() {
                            included.push(tag);
                        }
                    }
                    (
                        if included.is_empty() {
                            None
                        } else {
                            Some(included)
                        },
                        if excluded.is_empty() {
                            None
                        } else {
                            Some(excluded)
                        },
                    )
                })
                .unwrap_or((None, None));

            feature::get_all(
                &mut conn,
                &env,
                status,
                state,
                pattern,
                tags_included,
                tags_excluded,
            )
            .await?
        }
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
