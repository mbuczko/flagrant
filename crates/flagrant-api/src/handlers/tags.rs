use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, tag};
use flagrant_types::Tag;
use serde::Deserialize;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize)]
pub(crate) struct TagQueryParams {
    prefix: Option<String>,
}

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
