use axum::{extract::Path, Json};
use flagrant::models::{environment, identity};
use flagrant_types::FeatureResponse;

use crate::{errors::ServiceError, extractors::{DbConnection, Identity}};

pub async fn get_features(
    DbConnection(mut conn): DbConnection,
    Path(environment_id): Path<i32>,
    Identity(identity): Identity,
) -> Result<Json<Vec<FeatureResponse>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let variants = identity::get_variants(&mut conn, &env, identity)
        .await?
        .into_iter()
        .map(|v| {
            FeatureResponse {
                feature_id: v.feature_id,
                feature_name: v.name,
                value: v.value
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(variants))
}
