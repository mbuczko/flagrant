use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::project;
use flagrant_types::Project;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

pub async fn fetch(
    State(pool): State<SqlitePool>,
    Path(project_id): Path<u16>,
) -> Result<Json<Project>, ServiceError> {
    let mut conn = pool.acquire().await?;
    let project = project::get_by_id(&mut conn, project_id).await?;

    Ok(Json(project))
}
