use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, project};
use flagrant_types::{Environment, payload::EnvRequestPayload};
use serde::Deserialize;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize)]
pub struct EnvQueryParams {
    prefix: Option<String>,
}

pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
    Json(payload): Json<EnvRequestPayload>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::create(&mut conn, &project, payload.name, payload.description).await?;

    Ok(Json(env))
}

pub async fn fetch_by_id(
    DbConnection(mut conn): DbConnection,
    Path((_project_id, env_id)): Path<(i32, i32)>,
) -> Result<Json<Environment>, ServiceError> {
    let env = environment::get_by_id(&mut conn, env_id).await?;

    Ok(Json(env))
}

pub async fn fetch_by_name(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name)): Path<(i32, String)>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;

    Ok(Json(env))
}

pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<EnvQueryParams>,
    Path(project_id): Path<i32>,
) -> Result<Json<Vec<Environment>>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let envs = match params.prefix {
        Some(prefix) => environment::get_by_prefix(&mut conn, &project, prefix).await?,
        _ => environment::get_by_project(&mut conn, &project).await?,
    };

    Ok(Json(envs))
}
