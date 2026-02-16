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
    // Single environment lookup
    name: Option<String>,
    id: Option<i32>,
    // List filtering
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

/// Lists environments or fetches a single environment in a project.
///
/// When `name` or `id` query parameter is provided, fetches and returns a single environment.
/// Otherwise, lists environments with optional filtering.
///
/// # Endpoint
/// `GET /projects/{project_id}/envs?name=foo` - fetch by name
/// `GET /projects/{project_id}/envs?id=123` - fetch by ID
/// `GET /projects/{project_id}/envs?[prefix=...]` - list with filters
///
/// # Query Parameters
/// Single environment lookup (returns one environment in array):
/// - `name` - Environment name to fetch
/// - `id` - Environment ID to fetch (takes precedence over name)
///
/// List filtering:
/// - `prefix` - Filter by name prefix
///
/// # Returns
/// Array with single environment or list of environments matching the filters.
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<EnvQueryParams>,
    Path(project_id): Path<i32>,
) -> Result<Json<Vec<Environment>>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;

    // Check if this is a single-environment fetch request
    if params.id.is_some() || params.name.is_some() {
        let env = if let Some(id) = params.id {
            environment::get_by_id(&mut conn, id).await?
        } else if let Some(name) = params.name {
            environment::get_by_name(&mut conn, &project, name).await?
        } else {
            unreachable!()
        };
        return Ok(Json(vec![env]));
    }

    // Otherwise, list environments with filters
    let envs = match params.prefix {
        Some(prefix) => environment::get_by_prefix(&mut conn, &project, prefix).await?,
        _ => environment::get_by_project(&mut conn, &project).await?,
    };

    Ok(Json(envs))
}
