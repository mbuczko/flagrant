use axum::{Json, extract::Path};
use flagrant::models::project;
use flagrant_types::{
    Project,
    payload::{NewProjectPayload, ProjectCreatedResponse},
};

use crate::{errors::ServiceError, extractors::DbConnection};

/// Lists all projects.
#[utoipa::path(
    get,
    path = "/projects/",
    responses(
        (status = 200, description = "List of all projects", body = Vec<Project>)
    ),
    tag = "projects"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
) -> Result<Json<Vec<Project>>, ServiceError> {
    let projects = project::list(&mut conn).await?;

    Ok(Json(projects))
}

/// Fetches a project by name.
#[utoipa::path(
    get,
    path = "/projects/{project}",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    responses(
        (status = 200, description = "Project details", body = Project)
    ),
    tag = "projects"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
) -> Result<Json<Project>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;

    Ok(Json(project))
}

/// Creates a new project with a default environment.
#[utoipa::path(
    post,
    path = "/projects/",
    request_body = NewProjectPayload,
    responses(
        (status = 200, description = "Created project and its default environment", body = ProjectCreatedResponse)
    ),
    tag = "projects"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Json(payload): Json<NewProjectPayload>,
) -> Result<Json<ProjectCreatedResponse>, ServiceError> {
    let (project, environment) =
        project::create_with_environment(&mut conn, payload.name, None).await?;

    Ok(Json(ProjectCreatedResponse {
        project,
        environment,
    }))
}
