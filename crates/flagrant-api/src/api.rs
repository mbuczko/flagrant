use axum::{extract::{Path, State}, Json};
use flagrant::models::{environment, feature};
use flagrant_types::Feature;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

pub async fn get_feature(
    State(pool): State<SqlitePool>,
    Path((environment_id, feature_name)): Path<(u16, String)>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch_by_name(&pool, &env, feature_name).await?;

    Ok(Json(feature))
}
