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
use sqlx::SqliteConnection;

#[cfg(feature = "redis")]
use crate::cache::{CachedFeature, FeatureCache};
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
///
/// The result is cached (when a cache is configured) per project+environment+identity,
/// keyed independently of the srv-token, so a single cached entry serves both privileged
/// and unprivileged callers; srv-only filtering is applied at read time.
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
    Identity(identity_value): Identity,
    bearer: Option<TypedHeader<Authorization<Bearer>>>,
) -> Result<Json<Vec<FeatureResponse>>, ServiceError> {
    let bearer_token = bearer
        .as_ref()
        .map(|TypedHeader(Authorization(token))| token.token());
    let features = resolve_features(
        &state,
        &mut conn,
        project_name,
        env_name,
        identity_value,
        bearer_token,
    )
    .await?;

    Ok(Json(features))
}

/// Shared resolution logic behind both the HTTP (`get_features` above) and gRPC
/// (`grpc::GrpcFeatureResolver`) surfaces of the public feature-resolution endpoint -
/// srv-token gating, cache lookup/write and the DB fallback all live here once, so the two
/// transports can never drift in behavior.
pub async fn resolve_features(
    state: &AppState,
    conn: &mut SqliteConnection,
    project_name: String,
    env_name: String,
    identity_value: String,
    bearer_token: Option<&str>,
) -> anyhow::Result<Vec<FeatureResponse>> {
    let include_srv = {
        let config = state.config.read().unwrap();
        match (config.srv_token(&project_name, &env_name), bearer_token) {
            (Some(expected), Some(token)) => token == expected,
            _ => false,
        }
    };

    #[cfg(feature = "redis")]
    let cache_key = state
        .cache
        .as_ref()
        .map(|_| FeatureCache::key(&project_name, &env_name, &identity_value));

    #[cfg(feature = "redis")]
    if let (Some(cache), Some(key)) = (&state.cache, &cache_key)
        && let Some(cached) = cache.get(key).await
    {
        let response = cached
            .into_iter()
            // A valid token only ever adds srv-only features to the response - it never
            // narrows the response to just those.
            .filter(|f| include_srv || !f.is_srv)
            .map(|f| FeatureResponse {
                feature_id: f.feature_id,
                name: f.name,
                value: f.value,
                is_enabled: f.is_enabled,
                is_srv: f.is_srv,
            })
            .collect();
        return Ok(response);
    }

    let project = project::get_by_name(conn, project_name).await?;
    let env = environment::get_by_name(conn, &project, env_name).await?;
    let identity = identity::get_or_create_by_value(conn, &env, identity_value).await?;
    let all_variants = identity::get_identity_variants(conn, &env, &identity).await?;

    #[cfg(feature = "redis")]
    if let (Some(cache), Some(key)) = (&state.cache, &cache_key) {
        // Cache the unfiltered list (srv-only features included) so one entry serves
        // both privileged and unprivileged callers; filtering happens at read time.
        let cacheable: Vec<CachedFeature> = all_variants
            .into_iter()
            .filter_map(|v| {
                Some(CachedFeature {
                    feature_id: v.feature_id,
                    name: v.feature_name,
                    value: v.feature_value?,
                    is_srv: v.is_srv,
                    is_enabled: v.is_enabled,
                })
            })
            .collect();

        cache.set(key, &cacheable).await;

        let variants = cacheable
            .into_iter()
            .filter(|f| include_srv || !f.is_srv)
            .map(|f| FeatureResponse {
                feature_id: f.feature_id,
                name: f.name,
                value: f.value,
                is_enabled: f.is_enabled,
                is_srv: f.is_srv,
            })
            .collect();

        return Ok(variants);
    }

    let variants = all_variants
        .into_iter()
        .filter(|v| include_srv || !v.is_srv)
        .filter_map(|v| {
            Some(FeatureResponse {
                feature_id: v.feature_id,
                name: v.feature_name,
                value: v.feature_value?,
                is_enabled: v.is_enabled,
                is_srv: v.is_srv,
            })
        })
        .collect::<Vec<_>>();

    Ok(variants)
}
