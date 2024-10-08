use axum::{extract::Path, Json};
use flagrant::models::project;
use flagrant_types::Project;

use crate::{errors::ServiceError, extractors::DbConnection};

pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
) -> Result<Json<Project>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;

    Ok(Json(project))
}
