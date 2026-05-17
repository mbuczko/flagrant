use crate::{errors::ServiceError, extractors::DbConnection};
use axum::{Json, extract::Path};
use flagrant::models::{project, traits};
use flagrant_types::{Trait, payload::TraitRequestPayload};

/// Lists all defined traits.
#[utoipa::path(
    get,
    path = "/projects/{project_id}/traits",
    params(
        ("project_id" = i32, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "List of all traits", body = Vec<Trait>)
    ),
    tag = "traits"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
) -> Result<Json<Vec<Trait>>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let all = traits::get_all(&mut conn, &project).await?;

    Ok(Json(all))
}

/// Creates a new trait. If a trait with the same name already exists, returns it.
#[utoipa::path(
    post,
    path = "/projects/{project_id}/traits",
    params(
        ("project_id" = i32, Path, description = "Project ID")
    ),
    request_body = TraitRequestPayload,
    responses(
        (status = 200, description = "Created or existing trait", body = Trait)
    ),
    tag = "traits"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_id): Path<i32>,
    Json(payload): Json<TraitRequestPayload>,
) -> Result<Json<Trait>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let t = traits::upsert(&mut conn, &project, payload.name).await?;

    Ok(Json(t))
}

/// Deletes a trait and removes it from all identities it was attached to.
#[utoipa::path(
    delete,
    path = "/projects/{project_id}/traits/{trait_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("trait_id" = i32, Path, description = "Trait ID")
    ),
    responses(
        (status = 200, description = "Trait deleted")
    ),
    tag = "traits"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_id, trait_id)): Path<(i32, i32)>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;

    traits::delete(&mut conn, &project, trait_id).await?;
    Ok(Json(()))
}
