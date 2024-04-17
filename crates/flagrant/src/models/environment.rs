use hugsqlx::{params, HugSqlx};
use sqlx::{Pool, Sqlite};

use crate::errors::DbError;
use flagrant_types::{Environment, Project};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/environments.sql"]
struct Environments {}

pub async fn create(
    pool: &Pool<Sqlite>,
    project: &Project,
    name: String,
    description: Option<String>,
) -> anyhow::Result<Environment> {
    let env = Environments::create_environment::<_, Environment>(
        pool,
        params![
            project.id,
            name,
            description
        ],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create an environment");
        DbError::QueryFailed
    })?;

    Ok(env)
}

pub async fn fetch(pool: &Pool<Sqlite>, environment_id: u16) -> anyhow::Result<Environment> {
    let env = Environments::fetch_environment::<_, Environment>(pool, params![environment_id])
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not fetch environment");
            DbError::QueryFailed
        })?;

    Ok(env)
}

pub async fn fetch_by_name(
    pool: &Pool<Sqlite>,
    project: &Project,
    name: String,
) -> anyhow::Result<Environment> {
    let env = Environments::fetch_environment_by_name::<_, Environment>(
        pool,
        params![project.id, name],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch environment");
        DbError::QueryFailed
    })?;

    Ok(env)
}

pub async fn fetch_by_prefix(
    pool: &Pool<Sqlite>,
    project: &Project,
    prefix: String,
) -> anyhow::Result<Vec<Environment>> {
    let envs = Environments::fetch_environments_by_pattern::<_, Environment>(
        pool,
        params![project.id, format!("{}%", prefix)],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch list of environments");
        DbError::QueryFailed
    })?;

    Ok(envs)
}

pub async fn fetch_for_project(
    pool: &Pool<Sqlite>,
    project: &Project,
) -> anyhow::Result<Vec<Environment>> {
    let envs =
        Environments::fetch_environments_for_project::<_, Environment>(pool, params![project.id])
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch environments for project");
                DbError::QueryFailed
            })?;

    Ok(envs)
}
