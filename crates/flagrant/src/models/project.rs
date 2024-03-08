use hugsqlx::{params, HugSqlx};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

use crate::errors::DbError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/projects.sql"]
struct Projects {}

#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: u16,
    pub name: String,
}

pub async fn create_project(pool: &Pool<Sqlite>, name: String) -> anyhow::Result<Project> {
    Ok(
        Projects::create_project::<_, Project>(pool, params!(name))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could create a project");
                DbError::QueryFailed
            })?
    )
}

pub async fn fetch_project(
    pool: &Pool<Sqlite>,
    project_id: u16
) -> anyhow::Result<Project> {
    Ok(
        Projects::fetch_project::<_, Project>(pool, params!(project_id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could fetch project");
                DbError::QueryFailed
            })?,
    )
}
