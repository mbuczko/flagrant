use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, tag};
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
    path = "/envs/{environment_id}/tags",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
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
    Path(environment_id): Path<i32>,
) -> Result<Json<Vec<Tag>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let features = match params.prefix {
        Some(prefix) => tag::get_by_prefix(&mut conn, &env, prefix).await?,
        _ => tag::get_all(&mut conn, &env).await?,
    };

    Ok(Json(features))
}
