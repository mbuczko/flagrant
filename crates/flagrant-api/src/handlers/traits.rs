use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{project, traits};
use flagrant_types::{Trait, payload::NewTraitPayload};
use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct TraitQueryParams {
    /// Filter traits by name prefix
    prefix: Option<String>,
}

/// Lists all defined traits.
#[utoipa::path(
    get,
    path = "/projects/{project}/traits",
    params(
        ("project" = String, Path, description = "Project name"),
        TraitQueryParams
    ),
    responses(
        (status = 200, description = "List of all traits", body = Vec<Trait>)
    ),
    tag = "traits"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<TraitQueryParams>,
    Path(project_name): Path<String>,
) -> Result<Json<Vec<Trait>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let all = match super::parse_pattern(None, params.prefix) {
        Some(pattern) => traits::get_by_prefix(&mut conn, &project, pattern).await?,
        _ => traits::get_all(&mut conn, &project).await?,
    };

    Ok(Json(all))
}

/// Creates a new trait. If a trait with the same name already exists, returns it.
#[utoipa::path(
    post,
    path = "/projects/{project}/traits",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    request_body = NewTraitPayload,
    responses(
        (status = 200, description = "Created or existing trait", body = Trait)
    ),
    tag = "traits"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Json(payload): Json<NewTraitPayload>,
) -> Result<Json<Trait>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let t = traits::upsert(&mut conn, &project, payload.name).await?;

    Ok(Json(t))
}

/// Deletes a trait and removes it from all identities it was attached to.
#[utoipa::path(
    delete,
    path = "/projects/{project}/traits/{trait_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("trait_id" = i32, Path, description = "Trait ID")
    ),
    responses(
        (status = 200, description = "Trait deleted")
    ),
    tag = "traits"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_name, trait_id)): Path<(String, i32)>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;

    traits::delete(&mut conn, &project, trait_id).await?;
    Ok(Json(()))
}
