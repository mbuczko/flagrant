use hugsqlx::{params, HugSqlx};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

use crate::errors::DbError;

use super::project::Project;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/environments.sql"]
struct Environments {}

#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: u16,
    pub name: String,
    pub description: Option<String>,
}

pub async fn create_environment(
    pool: &Pool<Sqlite>,
    project: &Project,
    name: String,
    description: Option<String>,
) -> anyhow::Result<Environment> {
    Ok(Environments::create_environment::<_, Environment>(
        pool,
        params!(project.id, name, description),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create an environment");
        DbError::QueryFailed
    })?)
}

pub async fn fetch_environment(
    pool: &Pool<Sqlite>,
    environment_id: u16,
) -> anyhow::Result<Environment> {
    Ok(
        Environments::fetch_environment::<_, Environment>(pool, params!(environment_id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch environment");
                DbError::QueryFailed
            })?,
    )
}

pub async fn fetch_environment_by_name(
    prefix: &str,
    pool: &Pool<Sqlite>,
    project: &Project,
) -> anyhow::Result<Vec<Environment>> {
    Ok(Environments::fetch_environments_by_name::<_, Environment>(
        pool,
        params!(project.id, format!("{}%", prefix)),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch list of environments");
        DbError::QueryFailed
    })?)
}
