use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use sqlx::SqliteConnection;

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Project};

use super::environment;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/projects.sql"]
struct Projects {}

pub async fn create(conn: &mut SqliteConnection, name: String) -> anyhow::Result<Project> {
    let project: Project = Projects::create_project(conn, params!(name))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could create a project", e))?;

    project.validate()?;
    Ok(project)
}

pub async fn create_with_environment(
    conn: &mut SqliteConnection,
    project_name: String,
    environment_name: Option<String>,
) -> anyhow::Result<(Project, Environment)> {
    match create(conn, project_name).await {
        Ok(project) => {
            let env = environment::create(
                conn,
                &project,
                environment_name.unwrap_or_else(|| "base".to_string()),
                None,
                None,
            )
            .await
            .unwrap();

            Ok((project, env))
        }
        Err(err) => Err(err),
    }
}

pub async fn list(conn: &mut SqliteConnection) -> anyhow::Result<Vec<Project>> {
    let projects = Projects::list_projects(conn, params!())
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not list projects", e))?;

    Ok(projects)
}

pub async fn get_by_id(conn: &mut SqliteConnection, project_id: i32) -> anyhow::Result<Project> {
    let project = Projects::fetch_project(conn, params!(project_id))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could fetch project", e))?;

    Ok(project)
}

pub async fn get_by_name(conn: &mut SqliteConnection, name: String) -> anyhow::Result<Project> {
    let project = Projects::fetch_project_by_name(conn, params!(name))
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch project", e))?;

    Ok(project)
}
