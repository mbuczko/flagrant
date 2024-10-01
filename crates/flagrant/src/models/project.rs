use hugsqlx::{params, HugSqlx};
use sqlx::SqliteConnection;

use crate::errors::FlagrantError;
use flagrant_types::Project;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/projects.sql"]
struct Projects {}

pub async fn create(conn: &mut SqliteConnection, name: String) -> anyhow::Result<Project> {
    let project = Projects::create_project(conn, params!(name))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could create a project", e))?;

    Ok(project)
}

pub async fn get_by_id(conn: &mut SqliteConnection, project_id: u16) -> anyhow::Result<Project> {
    let project = Projects::fetch_project(conn, params!(project_id))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could fetch project", e))?;

    Ok(project)
}
