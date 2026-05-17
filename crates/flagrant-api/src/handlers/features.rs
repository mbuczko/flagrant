use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, feature, project};
use flagrant_types::{
    Feature,
    payload::{FeaturePatch, FeatureRequestPayload},
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::parsers;
use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct FeatureQueryParams {
    /// Filter by name prefix
    prefix: Option<String>,
    /// Filter by active status: "active" or "inactive"
    status: Option<String>,
    /// Filter by enabled state: "on" or "off"
    state: Option<String>,
    /// Comma-separated tags; prefix with `-` to exclude (e.g. "prod,-beta")
    tags: Option<String>,
    /// SQL LIKE pattern applied to feature names
    pattern: Option<String>,
}

#[derive(Debug)]
pub(crate) enum FeatureId {
    Id(i32),
    Name(String),
}

impl<'de> Deserialize<'de> for FeatureId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.parse::<i32>() {
            Ok(id) => Ok(FeatureId::Id(id)),
            Err(_) => Ok(FeatureId::Name(s)),
        }
    }
}

/// Creates a new feature in the specified environment.
///
/// The feature is created as inactive by default, with the enabled state
/// determined by the payload. The feature value becomes the environment's
/// control variant.
#[utoipa::path(
    post,
    path = "/projects/{project_id}/envs/{environment}/features",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name")
    ),
    request_body = FeatureRequestPayload,
    responses(
        (status = 200, description = "Created feature", body = Feature)
    ),
    tag = "features"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name)): Path<(i32, String)>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<Feature>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = feature::create(
        &mut conn,
        &env,
        payload.name,
        payload.description,
        payload.value,
        payload.is_enabled,
        false,
    )
    .await?;

    Ok(Json(feature))
}

/// Fetches a feature by its ID or name within a specific environment.
///
/// Returns the feature with all its variants (control and non-control).
#[utoipa::path(
    get,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name")
    ),
    responses(
        (status = 200, description = "Feature details with all variants", body = Feature)
    ),
    tag = "features"
)]
pub async fn fetch_by_id_or_name(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, feature_id)): Path<(i32, String, FeatureId)>,
) -> Result<Json<Feature>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = match feature_id {
        FeatureId::Id(id) => feature::get_by_id(&mut conn, &env, id).await?,
        FeatureId::Name(name) => feature::get_by_name(&mut conn, &env, name).await?,
    };
    Ok(Json(feature))
}

/// Updates an existing feature's name, value, and enabled state.
///
/// All updates are performed within a transaction. The feature value update
/// affects the environment's control variant.
#[utoipa::path(
    put,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    request_body = FeatureRequestPayload,
    responses(
        (status = 200, description = "Feature updated successfully")
    ),
    tag = "features"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, feature_id)): Path<(i32, String, i32)>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::update_one(&mut conn, &env, &feature)
        .name(payload.name)
        .value(payload.value)
        .enabled(payload.is_enabled)
        .update()
        .await?;

    Ok(Json(()))
}

/// Lists features optionally pre-filtered by name.
/// Each feature includes obligatory control variant and optional non-control ones.
///
/// # Query Parameters
/// - `pattern` - Filter by feature name substring (e.g., "banner" matches "show_banner", "show_banner_top")
/// - `prefix` - Filter by feature name prefix (e.g., "show_" matches "show_banner", "show_notification")
/// - `status` - Filter by active status: "active" or "inactive" (empty string ignored)
/// - `state` - Filter by enabled state: "on" or "off" (empty string ignored)
/// - `tags` - Comma-separated tags to filter by. Prefix with `-` to exclude (e.g., "prod,-beta")
#[utoipa::path(
    get,
    path = "/projects/{project_id}/envs/{environment}/features",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        FeatureQueryParams
    ),
    responses(
        (status = 200, description = "List of features with corresponding variants", body = Vec<Feature>)
    ),
    tag = "features"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<FeatureQueryParams>,
    Path((project_id, env_name)): Path<(i32, String)>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let (tags_included, tags_excluded) = parsers::parse_tags(params.tags.as_ref());
    let features = feature::get_all(
        &mut conn,
        &env,
        parsers::parse_status(params.status),
        parsers::parse_state(params.state),
        parsers::parse_pattern(params.pattern, params.prefix),
        tags_included,
        tags_excluded,
    )
    .await?;

    Ok(Json(features))
}

/// Deletes a feature and all its associated variants.
///
/// Deletion is performed within a transaction. All variants (including control
/// variants) are deleted before the feature itself.
#[utoipa::path(
    delete,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    responses(
        (status = 200, description = "Feature deleted successfully")
    ),
    tag = "features"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, feature_id)): Path<(i32, String, i32)>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::delete(&mut conn, &env, &feature).await?;
    Ok(Json(()))
}

/// Applies a batch of staged changes to a feature atomically.
///
/// All changes (feature properties and variant operations) are applied within
/// a single transaction. Validation errors are returned as 4xx responses.
#[utoipa::path(
    patch,
    path = "/projects/{project_id}/envs/{environment}/features/{feature_id}",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = i32, Path, description = "Feature ID")
    ),
    request_body = FeaturePatch,
    responses(
        (status = 200, description = "Patched feature with updated state", body = Feature)
    ),
    tag = "features"
)]
pub async fn patch(
    DbConnection(mut conn): DbConnection,
    Path((project_id, env_name, feature_id)): Path<(i32, String, i32)>,
    Json(patch): Json<FeaturePatch>,
) -> Result<Json<Feature>, ServiceError> {
    let project = project::get_by_id(&mut conn, project_id).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::apply_patch(&mut conn, &env, &feature, patch).await?;

    let updated = feature::get_by_id(&mut conn, &env, feature_id).await?;
    Ok(Json(updated))
}
