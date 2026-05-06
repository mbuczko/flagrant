use axum::{Json, extract::Path};
use flagrant::models::project;
use flagrant_types::{Environment, Project, payload::ProjectRequestPayload};

use crate::{errors::ServiceError, extractors::DbConnection};

pub async fn list(
    DbConnection(mut conn): DbConnection,
) -> Result<Json<Vec<Project>>, ServiceError> {
    let projects = project::list(&mut conn).await?;

    Ok(Json(projects))
}

pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
) -> Result<Json<Project>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;

    Ok(Json(project))
}

pub async fn create(
    DbConnection(mut conn): DbConnection,
    Json(payload): Json<ProjectRequestPayload>,
) -> Result<Json<(Project, Environment)>, ServiceError> {
    let (project, env) = project::create_with_environment(&mut conn, payload.name, None).await?;

    Ok(Json((project, env)))
}
