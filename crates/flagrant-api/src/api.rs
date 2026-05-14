use axum::{Json, extract::Path};
use flagrant::models::{environment, identity};
use flagrant_types::FeatureResponse;

use crate::{
    errors::ServiceError,
    extractors::{DbConnection, Identity},
};

/// Returns feature values for a given identity.
///
/// Requires the `X-Flagrant-Identity` header to identify the caller and
/// determine which variant value to return for each active feature.
#[utoipa::path(
    get,
    path = "/api/v1/envs/{environment_id}/features",
    params(
        ("environment_id" = i32, Path, description = "Environment ID"),
        ("X-Flagrant-Identity" = String, Header, description = "Caller identity used for variant assignment")
    ),
    responses(
        (status = 200, description = "Feature values for the identity", body = Vec<FeatureResponse>),
        (status = 401, description = "Missing X-Flagrant-Identity header")
    ),
    tag = "api"
)]
pub async fn get_features(
    DbConnection(mut conn): DbConnection,
    Path(environment_id): Path<i32>,
    Identity(identity): Identity,
) -> Result<Json<Vec<FeatureResponse>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let variants = identity::get_identity_variants(&mut conn, &env, identity)
        .await?
        .into_iter()
        .map(|v| FeatureResponse {
            feature_id: v.feature_id,
            name: v.feature_name,
            value: v.feature_value,
        })
        .collect::<Vec<_>>();

    Ok(Json(variants))
}
