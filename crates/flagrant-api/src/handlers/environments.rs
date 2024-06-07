use axum::{
    extract::{Path, Query, State},
    Json,
};
use flagrant::models::{environment, project};
use flagrant_types::{Environment, EnvRequestPayload};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

#[derive(Debug, Deserialize)]
pub struct EnvQueryParams {
    prefix: Option<String>,
}

pub async fn create(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
    Json(env): Json<EnvRequestPayload>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let env = environment::create(&pool, &project, env.name, env.description).await?;

    Ok(Json(env))
}

pub async fn fetch_by_id(
    State(pool): State<SqlitePool>,
    Path((_project_id, env_id)): Path<(u16, u16)>,
) -> Result<Json<Environment>, ServiceError> {
    let env = environment::fetch(&pool, env_id).await?;

    Ok(Json(env))
}

pub async fn fetch_by_name(
    State(pool): State<SqlitePool>,
    Path((project_id, env_name)): Path<(u16, String)>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let env = environment::fetch_by_name(&pool, &project, env_name).await?;

    Ok(Json(env))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Query(params): Query<EnvQueryParams>,
    Path(project_id): Path<u16>,
) -> Result<Json<Vec<Environment>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let envs = match params.prefix {
        Some(prefix) => environment::fetch_by_prefix(&pool, &project, prefix).await?,
        _ => environment::fetch_for_project(&pool, &project).await?
    };

    Ok(Json(envs))
}
