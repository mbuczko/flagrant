use hugsqlx::{params, HugSqlx};
use sqlx::{Pool, Sqlite};

use flagrant_types::Project;
use crate::errors::DbError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/projects.sql"]
struct Projects {}

pub async fn create(pool: &Pool<Sqlite>, name: String) -> anyhow::Result<Project> {
    Ok(
        Projects::create_project::<_, Project>(pool, params!(name))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could create a project");
                DbError::QueryFailed
            })?
    )
}

pub async fn fetch(
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
