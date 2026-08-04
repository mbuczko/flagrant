use axum::{
    Json,
    extract::{Path, State},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use flagrant::models::{environment, identity, project};
use flagrant_types::FeatureResponse;

use crate::{
    errors::ServiceError,
    extractors::{DbConnection, Identity},
    state::AppState,
};

/// Returns feature values for a given identity.
///
/// Requires the `X-Flagrant-Identity` header to identify the caller and
/// determine which variant value to return for each active feature. An optional
/// `Authorization: Bearer <token>` header, matching the srv-token configured for this
/// project+environment, additionally unlocks server-side-only ("srv") features - without
/// it (or with a non-matching token) those features are left out, everything else is
/// returned as usual.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project}/envs/{environment}/features",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("X-Flagrant-Identity" = String, Header, description = "Caller identity used for variant assignment"),
        ("Authorization" = Option<String>, Header, description = "Bearer token unlocking server-side-only features")
    ),
    responses(
        (status = 200, description = "Feature values for the identity", body = Vec<FeatureResponse>),
        (status = 401, description = "Missing X-Flagrant-Identity header")
    ),
    tag = "api"
)]
pub async fn get_features(
    State(state): State<AppState>,
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name)): Path<(String, String)>,
    Identity(identity): Identity,
    bearer: Option<TypedHeader<Authorization<Bearer>>>,
) -> Result<Json<Vec<FeatureResponse>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let identity = identity::get_or_create_by_value(&mut conn, &env, identity).await?;

    let include_srv = match (state.config.srv_token(&project.name, &env.name), &bearer) {
        (Some(expected), Some(TypedHeader(Authorization(token)))) => token.token() == expected,
        _ => false,
    };

    let variants = identity::get_identity_variants(&mut conn, &env, &identity)
        .await?
        .into_iter()
        // A valid token only ever *adds* srv-only features to the response - it never
        // narrows the response to just those.
        .filter(|v| include_srv || !v.is_srv)
        // get_identity_variants always distributes, so feature_value should always be Some.
        // filter_map drops any entries where distribution unexpectedly produced None.
        .filter_map(|v| {
            Some(FeatureResponse {
                feature_id: v.feature_id,
                name: v.feature_name,
                value: v.feature_value?,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(variants))
}
