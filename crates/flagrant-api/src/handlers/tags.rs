use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, project, tag};
use flagrant_types::Tag;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct TagQueryParams {
    /// Filter tags by name prefix
    prefix: Option<String>,
}

/// Lists tags for an environment with optional prefix filtering.
#[utoipa::path(
    get,
    path = "/projects/{project_id}/envs/{environment}/tags",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        TagQueryParams
    ),
    responses(
        (status = 200, description = "List of tags", body = Vec<Tag>)
    ),
    tag = "tags"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<TagQueryParams>,
    Path((project_id, env_name)): Path<(i32, String)>,
) -> Result<Json<Vec<Tag>>, ServiceError> {
    let proj = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &proj, env_name).await?;
    let features = match params.prefix {
        Some(prefix) => tag::get_by_prefix(&mut conn, &env, prefix).await?,
        _ => tag::get_all(&mut conn, &env).await?,
    };

    Ok(Json(features))
}
