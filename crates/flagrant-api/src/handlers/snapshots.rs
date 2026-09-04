use axum::{Json, extract::Path};
use flagrant::models::{environment, feature, project, snapshot};
use flagrant_types::{
    Snapshot, SnapshotDiff,
    payload::{RestoreRequest, UpdateSnapshotCommentPayload},
};

use crate::{
    errors::ServiceError,
    extractors::DbConnection,
    handlers::features::{FeatureId, resolve_feature_id},
};

/// Lists every snapshot for a feature within an environment, most recent first.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/snapshots",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name")
    ),
    responses(
        (status = 200, description = "Snapshots for this feature, most recent first", body = Vec<Snapshot>)
    ),
    tag = "snapshots"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id)): Path<(String, String, FeatureId)>,
) -> Result<Json<Vec<Snapshot>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature_id = resolve_feature_id(&mut conn, &env, feature_id).await?;

    let snapshots = snapshot::list(&mut conn, feature_id, env.id).await?;
    Ok(Json(snapshots))
}

/// Fetches a single snapshot by its version.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/snapshots/{version}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name"),
        ("version" = i32, Path, description = "Snapshot version")
    ),
    responses(
        (status = 200, description = "The requested snapshot", body = Snapshot)
    ),
    tag = "snapshots"
)]
pub async fn fetch(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, version)): Path<(String, String, FeatureId, i32)>,
) -> Result<Json<Snapshot>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature_id = resolve_feature_id(&mut conn, &env, feature_id).await?;

    let snapshot = snapshot::get_by_version(&mut conn, feature_id, env.id, version).await?;
    Ok(Json(snapshot))
}

/// Updates a snapshot's comment in place - the only field of a recorded snapshot that's
/// ever mutated after the fact.
#[utoipa::path(
    patch,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/snapshots/{version}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name"),
        ("version" = i32, Path, description = "Snapshot version")
    ),
    request_body = UpdateSnapshotCommentPayload,
    responses(
        (status = 200, description = "The snapshot with its updated comment", body = Snapshot)
    ),
    tag = "snapshots"
)]
pub async fn update_comment(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, version)): Path<(String, String, FeatureId, i32)>,
    Json(payload): Json<UpdateSnapshotCommentPayload>,
) -> Result<Json<Snapshot>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature_id = resolve_feature_id(&mut conn, &env, feature_id).await?;

    let snapshot =
        snapshot::set_comment(&mut conn, feature_id, env.id, version, payload.comment).await?;
    Ok(Json(snapshot))
}

/// Compares a feature's current live state against a snapshot version, previewing
/// what a `restore` to that version would change.
#[utoipa::path(
    get,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/snapshots/{version}/diff",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name"),
        ("version" = i32, Path, description = "Snapshot version to diff against")
    ),
    responses(
        (status = 200, description = "The feature's current state alongside the target snapshot", body = SnapshotDiff)
    ),
    tag = "snapshots"
)]
pub async fn diff(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, version)): Path<(String, String, FeatureId, i32)>,
) -> Result<Json<SnapshotDiff>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = match feature_id {
        FeatureId::Id(id) => feature::get_by_id(&mut conn, &env, id).await?,
        FeatureId::Name(name) => feature::get_by_name(&mut conn, &env, &name).await?,
    };

    let diff = snapshot::diff(&mut conn, &project, &env, &feature, version).await?;
    Ok(Json(diff))
}

/// Restores a feature to the state captured by a given snapshot version.
///
/// Restoring is itself a commit: it produces a brand-new snapshot whose state matches the
/// target version, rather than rewriting history in place. See [`flagrant::models::snapshot::restore`].
#[utoipa::path(
    post,
    path = "/projects/{project}/envs/{environment}/features/{feature_id}/snapshots/{version}/restore",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name"),
        ("feature_id" = String, Path, description = "Feature ID or name"),
        ("version" = i32, Path, description = "Snapshot version to restore to")
    ),
    request_body = RestoreRequest,
    responses(
        (status = 200, description = "The new snapshot produced by the restore", body = Snapshot)
    ),
    tag = "snapshots"
)]
pub async fn restore(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name, feature_id, version)): Path<(String, String, FeatureId, i32)>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<Snapshot>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;
    let feature = match feature_id {
        FeatureId::Id(id) => feature::get_by_id(&mut conn, &env, id).await?,
        FeatureId::Name(name) => feature::get_by_name(&mut conn, &env, &name).await?,
    };

    let snapshot = snapshot::restore(
        &mut conn,
        &project,
        &env,
        &feature,
        version,
        payload.comment,
    )
    .await?;
    Ok(Json(snapshot))
}
