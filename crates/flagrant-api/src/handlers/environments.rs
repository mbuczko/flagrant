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

#[derive(Debug)]
pub(crate) enum EnvironmentId {
    Id(i32),
    Name(String),
}

impl<'de> Deserialize<'de> for EnvironmentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.parse::<i32>() {
            Ok(id) => Ok(EnvironmentId::Id(id)),
            Err(_) => Ok(EnvironmentId::Name(s)),
        }
    }
}

pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
    Json(payload): Json<EnvRequestPayload>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::create(&mut conn, &project, payload.name, payload.description, payload.base_env).await?;

    Ok(Json(env))
}

pub async fn fetch_by_id_or_name(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_id)): Path<(i32, EnvironmentId)>,
) -> Result<Json<Environment>, ServiceError> {
    let env = match env_id {
        EnvironmentId::Id(id) => environment::get_by_id(&mut conn, id).await?,
        EnvironmentId::Name(name) => {
            let project = project::get_by_id(&mut conn, project_id).await?;
            environment::get_by_name(&mut conn, &project, name).await?
        }
    };
    Ok(Json(env))
}

/// Lists environments with optional filtering.
///
/// # Endpoint
/// `GET /projects/{project_id}/envs?[prefix=...]` - list with filters
///
/// # Query Parameters
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
    let envs = match params.prefix {
        Some(prefix) => environment::get_by_prefix(&mut conn, &project, prefix).await?,
        _ => environment::get_by_project(&mut conn, &project).await?,
    };

    Ok(Json(envs))
}
