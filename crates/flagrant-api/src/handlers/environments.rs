use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, project};
use flagrant_types::{Environment, payload::NewEnvironmentPayload};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize, IntoParams)]
pub struct EnvQueryParams {
    /// Filter by environment name prefix
    prefix: Option<String>,
    /// Optional pattern to filter environments (substring match)
    pattern: Option<String>,
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

/// Creates a new environment within a project.
#[utoipa::path(
    post,
    path = "/projects/{project}/envs",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    request_body = NewEnvironmentPayload,
    responses(
        (status = 200, description = "Created environment", body = Environment)
    ),
    tag = "environments"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Json(payload): Json<NewEnvironmentPayload>,
) -> Result<Json<Environment>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::create(
        &mut conn,
        &project,
        payload.name,
        payload.description,
        payload.base_env,
    )
    .await?;

    Ok(Json(env))
}

/// Fetches an environment by its ID or name within a project.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{env_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("env_id" = String, Path, description = "Environment ID or name")
    ),
    responses(
        (status = 200, description = "Environment details", body = Environment)
    ),
    tag = "environments"
)]
pub async fn fetch_by_id_or_name(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_id)): Path<(String, EnvironmentId)>,
) -> Result<Json<Environment>, ServiceError> {
    let env = match env_id {
        EnvironmentId::Id(id) => environment::get_by_id(&mut conn, id).await?,
        EnvironmentId::Name(name) => {
            let project = project::get_by_name(&mut conn, project_name).await?;
            environment::get_by_name(&mut conn, &project, name).await?
        }
    };
    Ok(Json(env))
}

/// Lists environments with optional filtering.
///
/// # Endpoint
/// `GET /projects/{project}/envs?[prefix=...]` - list with filters
///
/// # Query Parameters
/// - `prefix` - Filter by name prefix (anchored to start)
/// - `pattern` - Filter by name substring (takes precedence over prefix)
///
/// # Returns
/// Array with single environment or list of environments matching the filters.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs",
    params(
        ("project" = String, Path, description = "Project name"),
        EnvQueryParams
    ),
    responses(
        (status = 200, description = "List of environments", body = Vec<Environment>)
    ),
    tag = "environments"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<EnvQueryParams>,
    Path(project_name): Path<String>,
) -> Result<Json<Vec<Environment>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let envs = environment::list(
        &mut conn,
        &project,
        super::parse_pattern(params.pattern, params.prefix),
    )
    .await?;

    Ok(Json(envs))
}
