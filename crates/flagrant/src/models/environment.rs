use hugsqlx::{HugSqlx, params};
use sqlx::SqliteConnection;

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Project};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/environments.sql"]
struct SQLEnvironments {}

pub async fn create(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
    description: Option<String>,
) -> anyhow::Result<Environment> {
    let env = SQLEnvironments::create_environment(conn, params![project.id, name, description])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not create an environment", e))?;

    Ok(env)
}

pub async fn get_by_id(
    conn: &mut SqliteConnection,
    environment_id: i32,
) -> anyhow::Result<Environment> {
    let env = SQLEnvironments::fetch_environment(conn, params![environment_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch environment", e))?;

    Ok(env)
}

pub async fn get_by_name(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
) -> anyhow::Result<Environment> {
    let env = SQLEnvironments::fetch_environment_by_name(conn, params![project.id, name])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch environment", e))?;

    Ok(env)
}

pub async fn get_by_prefix(
    conn: &mut SqliteConnection,
    project: &Project,
    prefix: String,
) -> anyhow::Result<Vec<Environment>> {
    let envs = SQLEnvironments::fetch_environments_by_pattern::<_, Environment>(
        conn,
        params![project.id, format!("{}%", prefix)],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of environments", e))?;

    Ok(envs)
}

pub async fn get_by_project(
    conn: &mut SqliteConnection,
    project: &Project,
) -> anyhow::Result<Vec<Environment>> {
    let envs = SQLEnvironments::fetch_environments_for_project(conn, params![project.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of environments", e))?;

    Ok(envs)
}
