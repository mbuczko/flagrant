use axum::{
    Json,
    extract::{Path, Query},
};
use flagrant::models::{project, segment};
use flagrant_types::{
    Project, Segment, SegmentFeatureOverride,
    payload::{NewSegmentPayload, SegmentVariantWeight},
};
use serde::Deserialize;
use sqlx::SqliteConnection;
use utoipa::IntoParams;

use crate::{errors::ServiceError, extractors::DbConnection};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct SegmentQueryParams {
    prefix: Option<String>,
    pattern: Option<String>,
}

#[derive(Debug)]
pub(crate) enum SegmentId {
    Id(i32),
    Name(String),
}

impl<'de> Deserialize<'de> for SegmentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.parse::<i32>() {
            Ok(id) => Ok(SegmentId::Id(id)),
            Err(_) => Ok(SegmentId::Name(s)),
        }
    }
}

async fn resolve_segment(
    conn: &mut SqliteConnection,
    project: &Project,
    segment_id: SegmentId,
) -> anyhow::Result<Segment> {
    match segment_id {
        SegmentId::Id(id) => segment::get_by_id(conn, project, id).await,
        SegmentId::Name(name) => segment::get_by_name(conn, project, &name).await,
    }
}

/// Lists all segments for the given project.
#[utoipa::path(
    get,
    path = "/projects/{project}/segments",
    params(
        ("project" = String, Path, description = "Project name"),
        SegmentQueryParams
    ),
    responses(
        (status = 200, description = "List of segments", body = Vec<Segment>)
    ),
    tag = "segments"
)]
pub async fn list(
    DbConnection(mut conn): DbConnection,
    Query(params): Query<SegmentQueryParams>,
    Path(project_name): Path<String>,
) -> Result<Json<Vec<Segment>>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let segments = segment::get_all(
        &mut conn,
        &project,
        super::parse_pattern(params.pattern, params.prefix),
    )
    .await?;
    Ok(Json(segments))
}

/// Creates a new segment in the given project.
#[utoipa::path(
    post,
    path = "/projects/{project}/segments",
    params(
        ("project" = String, Path, description = "Project name")
    ),
    request_body = NewSegmentPayload,
    responses(
        (status = 200, description = "Created segment", body = Segment)
    ),
    tag = "segments"
)]
pub async fn create(
    DbConnection(mut conn): DbConnection,
    Path(project_name): Path<String>,
    Json(payload): Json<NewSegmentPayload>,
) -> Result<Json<Segment>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = segment::create(&mut conn, &project, payload.name, payload.description).await?;
    Ok(Json(seg))
}

/// Fetches a segment by ID or name.
#[utoipa::path(
    get,
    path = "/projects/{project}/segments/{segment_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name")
    ),
    responses(
        (status = 200, description = "Segment details", body = Segment)
    ),
    tag = "segments"
)]
pub async fn fetch_by_id_or_name(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id)): Path<(String, SegmentId)>,
) -> Result<Json<Segment>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    Ok(Json(seg))
}

/// Returns stored variant weight overrides for a segment+feature+environment.
#[utoipa::path(
    get,
    path = "/projects/{project}/segments/{segment_id}/features/{feature_id}/overrides/{environment_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name"),
        ("feature_id" = i32, Path, description = "Feature ID"),
        ("environment_id" = i32, Path, description = "Environment ID")
    ),
    responses(
        (status = 200, description = "Stored variant weight overrides", body = Vec<SegmentVariantWeight>)
    ),
    tag = "segments"
)]
pub async fn get_feature_override_weights(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id, feature_id, environment_id)): Path<(String, i32, i32, i32)>,
) -> Result<Json<Vec<SegmentVariantWeight>>, ServiceError> {
    let _project = project::get_by_name(&mut conn, project_name).await?;
    let rows =
        segment::get_variant_weights(&mut conn, segment_id, feature_id, environment_id).await?;
    let weights = rows
        .into_iter()
        .map(|(variant_id, weight)| SegmentVariantWeight { variant_id, weight })
        .collect();

    Ok(Json(weights))
}

/// Returns every feature a segment overrides within a given environment, each with its
/// full weight breakdown (including the control variant's auto-balanced remainder).
#[utoipa::path(
    get,
    path = "/projects/{project}/segments/{segment_id}/overrides/{environment_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name"),
        ("environment_id" = i32, Path, description = "Environment ID")
    ),
    responses(
        (status = 200, description = "Features overridden by this segment, with full weight breakdowns", body = Vec<SegmentFeatureOverride>)
    ),
    tag = "segments"
)]
pub async fn get_overridden_features(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id, environment_id)): Path<(String, i32, i32)>,
) -> Result<Json<Vec<SegmentFeatureOverride>>, ServiceError> {
    let _project = project::get_by_name(&mut conn, project_name).await?;
    let overrides =
        segment::list_overridden_features(&mut conn, environment_id, segment_id).await?;

    Ok(Json(overrides))
}
