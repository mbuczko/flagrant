use axum::{
    extract::{Path, Query, State},
    Json,
};
use flagrant::models::{environment, project};
use flagrant_types::{NewEnvRequestPayload, Environment};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    name: Option<String>,
}

pub async fn fetch(
    State(pool): State<SqlitePool>,
    Path((project_id, env_name)): Path<(u16, String)>,
) -> Result<Json<Option<Environment>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    Ok(Json(environment::fetch_by_name(&pool, &project, env_name).await?))
}

pub async fn list(
    State(pool): State<SqlitePool>,
    Query(params): Query<QueryParams>,
    Path(project_id): Path<u16>,
) -> Result<Json<Vec<Environment>>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    if let Some(pattern) = params.name {
        Ok(Json(environment::fetch_by_pattern(&pool, &project, pattern).await?))
    } else {
        Ok(Json(environment::fetch_for_project(&pool, &project).await?))
    }
}

pub async fn create(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
    Json(env): Json<NewEnvRequestPayload>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::fetch(&pool, project_id).await?;
    let env = environment::create(&pool, &project, env.name, env.description).await?;

    Ok(Json(env))
}
