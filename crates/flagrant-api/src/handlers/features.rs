use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{environment, feature};
use flagrant_types::{
    Feature,
    payload::{FeaturePatch, FeatureRequestPayload},
};
use serde::Deserialize;
use smallvec::{SmallVec, smallvec};

use crate::{errors::ServiceError, extractors::DbConnection};

type TagsTuple<'a> = (
    Option<SmallVec<[&'a str; 3]>>, // Tags included
    Option<SmallVec<[&'a str; 3]>>, // Tags excluded
);

#[derive(Debug, Deserialize)]
pub(crate) struct FeatureQueryParams {
    prefix: Option<String>,
    status: Option<String>,
    state: Option<String>,
    tags: Option<String>,
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

/// Parses status parameter: converts non-empty string to bool (true if "active").
fn parse_status(status: Option<String>) -> Option<bool> {
    status.filter(|s| !s.is_empty()).map(|s| s == "active")
}

/// Parses state parameter: converts non-empty string to bool (true if "on").
fn parse_state(state: Option<String>) -> Option<bool> {
    state.filter(|s| !s.is_empty()).map(|s| s == "on")
}

/// Parses pattern parameter: wraps non-empty string with SQL wildcards.
fn parse_pattern(pattern: Option<String>) -> Option<String> {
    pattern.filter(|s| !s.is_empty()).map(|p| format!("%{p}%"))
}

/// Parses tags parameter into included and excluded tag lists.
/// Tags prefixed with '-' are excluded, others are included.
fn parse_tags<'a>(tags: Option<&'a String>) -> TagsTuple<'a> {
    tags.map(|tags| {
        let (mut included, mut excluded) = (smallvec![], smallvec![]);

        for tag in tags.split(',') {
            if let Some(tag) = tag.strip_prefix('-')
                && !tag.is_empty()
            {
                excluded.push(tag);
            } else if !tag.is_empty() {
                included.push(tag);
            }
        }

        (
            if included.is_empty() {
                None
            } else {
                Some(included)
            },
            if excluded.is_empty() {
                None
            } else {
                Some(excluded)
            },
        )
    })
    .unwrap_or((None, None))
}

/// Creates a new feature in the specified environment.
///
/// The feature is created as inactive by default, with the enabled state
/// determined by the payload. The feature value becomes the environment's
/// control variant.
///
/// # Endpoint
/// `POST /environments/{environment_id}/features`
///
/// # Returns
/// The newly created feature with its control variant.
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(environment_id): Path<i32>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::create(
        &mut conn,
        &env,
        payload.name,
        payload.value,
        payload.is_enabled,
        false,
    )
    .await?;

    Ok(Json(feature))
}

/// Fetches a feature by its ID within a specific environment.
///
/// Returns the feature with all its variants (control and non-control).
///
/// # Endpoint
/// `GET /environments/{environment_id}/features/{feature_id}`
///
/// # Returns
/// The feature with all its associated variants.
pub async fn fetch_by_id_or_name(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, FeatureId)>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
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
///
/// # Endpoint
/// `PUT /environments/{environment_id}/features/{feature_id}`
///
/// # Parameters
/// - `environment_id` - The environment containing the feature
/// - `feature_id` - The ID of the feature to update
/// - `payload` - The new feature properties (name, value, is_enabled)
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
    Json(payload): Json<FeatureRequestPayload>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::update_one(&mut conn, &env, &feature)
        .name(payload.name)
        .value(payload.value)
        .enabled(payload.is_enabled)
        .update()
        .await?;

    Ok(Json(()))
}

/// Lists features with optional filtering.
/// Listed features are returned with their control variants only for performance reasons.
///
/// # Endpoint
/// `GET /environments/{environment_id}/features?[prefix=...][status=...][state=...][pattern=...][tags=...]` - list with filters
///
/// # Query Parameters
/// - `prefix` - Filter by name prefix (e.g., "show_" matches "show_banner", "show_notification")
/// - `status` - Filter by active status: "active" or "inactive" (empty string ignored)
/// - `state` - Filter by enabled state: "on" or "off" (empty string ignored)
/// - `pattern` - SQL LIKE pattern search on feature names (wildcards added automatically)
/// - `tags` - Comma-separated tags to filter by. Prefix with `-` to exclude (e.g., "prod,-beta")
///
/// # Returns
/// Array with single feature or list of features matching the filters, each with only its control variant.
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<FeatureQueryParams>,
    Path(environment_id): Path<i32>,
) -> Result<Json<Vec<Feature>>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let features = match params.prefix {
        // TODO: get_by_prefix is unnecessary - reuse get_all with an additional (prefix) parameter
        Some(prefix) => feature::get_by_prefix(&mut conn, &env, prefix).await?,
        None => {
            let (tags_included, tags_excluded) = parse_tags(params.tags.as_ref());

            feature::get_all(
                &mut conn,
                &env,
                parse_status(params.status),
                parse_state(params.state),
                parse_pattern(params.pattern),
                tags_included,
                tags_excluded,
            )
            .await?
        }
    };

    Ok(Json(features))
}

/// Deletes a feature and all its associated variants.
///
/// Deletion is performed within a transaction. All variants (including control
/// variants) are deleted before the feature itself. Control variants are deleted
/// last due to strict deletion policy.
///
/// # Endpoint
/// `DELETE /environments/{environment_id}/features/{feature_id}`
///
/// # Parameters
/// - `environment_id` - The environment containing the feature
/// - `feature_id` - The ID of the feature to delete
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
) -> Result<Json<()>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::delete(&mut conn, &env, &feature).await?;
    Ok(Json(()))
}

/// Applies a batch of staged changes to a feature atomically.
///
/// All changes (feature properties and variant operations) are applied within
/// a single transaction. Validation errors are returned as 4xx responses.
///
/// # Endpoint
/// `POST /environments/{environment_id}/features/{feature_id}/patch`
///
/// # Parameters
/// - `environment_id` - The environment containing the feature
/// - `feature_id` - The ID of the feature to patch
/// - `patch` - The set of changes to apply
pub async fn patch(
    DbConnection(mut conn): DbConnection,
    Path((environment_id, feature_id)): Path<(i32, i32)>,
    Json(patch): Json<FeaturePatch>,
) -> Result<Json<Feature>, ServiceError> {
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let feature = feature::get_by_id(&mut conn, &env, feature_id).await?;

    feature::apply_patch(&mut conn, &env, &feature, patch).await?;

    let updated = feature::get_by_id(&mut conn, &env, feature_id).await?;
    Ok(Json(updated))
}
