use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, identity};
use flagrant_types::IdentityVariant;
use sqlx::SqlitePool;

use crate::{errors::ServiceError, extractors::Identity};

pub async fn get_features(
    State(pool): State<SqlitePool>,
    Path(environment_id): Path<u16>,
    Identity(identity): Identity,
) -> Result<Json<Vec<IdentityVariant>>, ServiceError> {
    let mut conn = pool.acquire().await?;
    let env = environment::get_by_id(&mut conn, environment_id).await?;

    let variants = identity::get_variants(&mut conn, &env, identity).await?;
    Ok(Json(variants))
}
