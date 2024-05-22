use hugsqlx::{params, HugSqlx};
use sqlx::{Pool, Sqlite};

use crate::errors::FlagrantError;
use flagrant_types::Project;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/projects.sql"]
struct Projects {}

pub async fn create(pool: &Pool<Sqlite>, name: String) -> anyhow::Result<Project> {
    let project = Projects::create_project(pool, params!(name))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could create a project", e))?;

    Ok(project)
}

pub async fn fetch(pool: &Pool<Sqlite>, project_id: u16) -> anyhow::Result<Project> {
    let project = Projects::fetch_project(pool, params!(project_id))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could fetch project", e))?;

    Ok(project)
}
