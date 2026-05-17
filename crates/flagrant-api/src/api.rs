use axum::{Json, extract::Path};
use flagrant::models::{environment, identity, project};
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
    path = "/api/v1/projects/{project_id}/envs/{environment}/features",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
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
    Path((project_id, env_name)): Path<(i32, String)>,
    Identity(identity): Identity,
) -> Result<Json<Vec<FeatureResponse>>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let variants = identity::get_identity_variants(&mut conn, &project, &env, identity)
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
