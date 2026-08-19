use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::{
    errors::FlagrantError,
    models::{environment, project},
};
use flagrant_types::{
    Environment,
    payload::{NewEnvironmentPayload, UpdateEnvironmentPayload},
};
use serde::Deserialize;
use sqlx::SqliteConnection;
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
    let env = resolve_environment(&mut conn, project_name, env_id).await?;
    Ok(Json(env))
}

/// Resolves an `EnvironmentId` (numeric or name) to an `Environment`, always scoped to
/// `project_name` - unlike a bare `environment::get_by_id`, which looks up an environment
/// by id globally with no project check at all. Without this, a numeric id belonging to a
/// *different* project would silently resolve (e.g. `/projects/demo/envs/1` returning
/// project `sample`'s environment 1, just because the id happens to exist), letting a
/// session end up scoped to the wrong project's environment while believing it's on the
/// named one.
async fn resolve_environment(
    conn: &mut SqliteConnection,
    project_name: String,
    env_id: EnvironmentId,
) -> anyhow::Result<Environment> {
    let project = project::get_by_name(conn, project_name).await?;
    match env_id {
        EnvironmentId::Id(id) => {
            let env = environment::get_by_id(conn, id).await?;
            if env.project_id != project.id {
                return Err(FlagrantError::NotFound(
                    "No environment of given id found in this project",
                )
                .into());
            }
            Ok(env)
        }
        EnvironmentId::Name(name) => environment::get_by_name(conn, &project, name).await,
    }
}

/// Updates an environment's description.
#[utoipa::path(
    put,
    path = "/projects/{project}/envs/{env_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("env_id" = String, Path, description = "Environment ID or name")
    ),
    request_body = UpdateEnvironmentPayload,
    responses(
        (status = 200, description = "Environment updated")
    ),
    tag = "environments"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_id)): Path<(String, EnvironmentId)>,
    Json(payload): Json<UpdateEnvironmentPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = resolve_environment(&mut conn, project_name, env_id).await?;
    environment::update(&mut conn, &env, payload.description.as_deref()).await?;
    Ok(Json(()))
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
