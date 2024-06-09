use axum::{extract::{Path, State}, Json};
use flagrant::{distributor, models::{environment, feature}};
use flagrant_types::FeatureValue;
use sqlx::SqlitePool;

use crate::errors::ServiceError;

pub async fn get_feature(
    State(pool): State<SqlitePool>,
    Path((environment_id, _ident, feature_name)): Path<(u16, String, String)>,
) -> Result<Json<FeatureValue>, ServiceError> {
    let env = environment::fetch(&pool, environment_id).await?;
    let feature = feature::fetch_by_name(&pool, &env, feature_name).await?;
    let variant = distributor::Distributor::new(feature).distribute(&pool, &env).await?;

    Ok(Json(variant.value))
}
