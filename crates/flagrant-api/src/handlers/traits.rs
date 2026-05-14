use axum::{Json, extract::Path};
use flagrant::models::traits;
use flagrant_types::{Trait, payload::TraitRequestPayload};
use crate::{errors::ServiceError, extractors::DbConnection};

/// Lists all defined traits.
#[utoipa::path(
    get,
    path = "/traits",
    responses(
        (status = 200, description = "List of all traits", body = Vec<Trait>)
    ),
    tag = "traits"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
) -> Result<Json<Vec<Trait>>, ServiceError> {
    let all = traits::get_all(&mut conn).await?;
    Ok(Json(all))
}

/// Creates a new trait. If a trait with the same name already exists, returns it.
#[utoipa::path(
    post,
    path = "/traits",
    request_body = TraitRequestPayload,
    responses(
        (status = 200, description = "Created or existing trait", body = Trait)
    ),
    tag = "traits"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Json(payload): Json<TraitRequestPayload>,
) -> Result<Json<Trait>, ServiceError> {
    let t = traits::upsert(&mut conn, payload.name).await?;
    Ok(Json(t))
}

/// Deletes a trait and removes it from all identities it was attached to.
#[utoipa::path(
    delete,
    path = "/traits/{trait_id}",
    params(
        ("trait_id" = i32, Path, description = "Trait ID")
    ),
    responses(
        (status = 200, description = "Trait deleted")
    ),
    tag = "traits"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path(trait_id): Path<i32>,
) -> Result<Json<()>, ServiceError> {
    traits::delete(&mut conn, trait_id).await?;
    Ok(Json(()))
}
