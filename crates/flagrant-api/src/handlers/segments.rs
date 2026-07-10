use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use flagrant::models::{project, rule, segment};
use flagrant_types::{
    Project, Segment, SegmentGroup, SegmentRule,
    payload::{
        NewGroupPayload, NewRulePayload, NewSegmentPayload, SegmentPatch, SegmentVariantWeight,
    },
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
        SegmentId::Name(name) => segment::get_by_name(conn, project, name).await,
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

/// Updates a segment's name and description.
#[utoipa::path(
    put,
    path = "/projects/{project}/segments/{segment_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name")
    ),
    request_body = NewSegmentPayload,
    responses(
        (status = 200, description = "Segment updated")
    ),
    tag = "segments"
)]
pub async fn update(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id)): Path<(String, SegmentId)>,
    Json(payload): Json<NewSegmentPayload>,
) -> Result<Json<()>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    segment::update(
        &mut conn,
        &seg,
        &payload.name,
        payload.description.as_deref(),
    )
    .await?;
    Ok(Json(()))
}

/// Deletes a segment and all its groups and rules.
#[utoipa::path(
    delete,
    path = "/projects/{project}/segments/{segment_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name")
    ),
    responses(
        (status = 200, description = "Segment deleted")
    ),
    tag = "segments"
)]
pub async fn delete(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id)): Path<(String, SegmentId)>,
) -> Result<StatusCode, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    segment::delete(&mut conn, &seg).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Applies a batch of staged operations to a segment.
pub async fn patch_segment(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id)): Path<(String, SegmentId)>,
    Json(payload): Json<SegmentPatch>,
) -> Result<Json<Segment>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    let updated = segment::patch(&mut conn, &project, seg, payload).await?;
    Ok(Json(updated))
}

/// Adds a group to a segment.
///
/// The first group added is the head (connector must be omitted or null).
/// Subsequent groups require a connector (`and` or `and_not`).
#[utoipa::path(
    post,
    path = "/projects/{project}/segments/{segment_id}/groups",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name")
    ),
    request_body = NewGroupPayload,
    responses(
        (status = 200, description = "Added group", body = SegmentGroup)
    ),
    tag = "segments"
)]
pub async fn add_group(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id)): Path<(String, SegmentId)>,
    Json(payload): Json<NewGroupPayload>,
) -> Result<Json<SegmentGroup>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    let group = segment::add_group(&mut conn, &seg, payload.description, payload.connector).await?;
    Ok(Json(group))
}

/// Removes a group and all its rules from a segment.
#[utoipa::path(
    delete,
    path = "/projects/{project}/segments/{segment_id}/groups/{group_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name"),
        ("group_id" = i32, Path, description = "Group ID")
    ),
    responses(
        (status = 200, description = "Group deleted")
    ),
    tag = "segments"
)]
pub async fn delete_group(
    DbConnection(mut conn): DbConnection,
    Path((project_name, segment_id, group_id)): Path<(String, SegmentId, i32)>,
) -> Result<StatusCode, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let seg = resolve_segment(&mut conn, &project, segment_id).await?;
    segment::delete_group(&mut conn, &seg, group_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Adds a rule to a group.
#[utoipa::path(
    post,
    path = "/projects/{project}/segments/{segment_id}/groups/{group_id}/rules",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name"),
        ("group_id" = i32, Path, description = "Group ID")
    ),
    request_body = NewRulePayload,
    responses(
        (status = 200, description = "Added rule", body = SegmentRule)
    ),
    tag = "segments"
)]
pub async fn add_rule(
    DbConnection(mut conn): DbConnection,
    Path((project_name, _segment_id, group_id)): Path<(String, SegmentId, i32)>,
    Json(payload): Json<NewRulePayload>,
) -> Result<Json<SegmentRule>, ServiceError> {
    let _project = project::get_by_name(&mut conn, project_name).await?;
    let rule = rule::add(
        &mut conn,
        group_id,
        payload.driver,
        payload.comparator,
        payload.value,
    )
    .await?;
    Ok(Json(rule))
}

/// Removes a rule from a group.
#[utoipa::path(
    delete,
    path = "/projects/{project}/segments/{segment_id}/groups/{group_id}/rules/{rule_id}",
    params(
        ("project" = String, Path, description = "Project name"),
        ("segment_id" = String, Path, description = "Segment ID or name"),
        ("group_id" = i32, Path, description = "Group ID"),
        ("rule_id" = i32, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule deleted")
    ),
    tag = "segments"
)]
pub async fn delete_rule(
    DbConnection(mut conn): DbConnection,
    Path((project_name, _segment_id, _group_id, rule_id)): Path<(String, SegmentId, i32, i32)>,
) -> Result<StatusCode, ServiceError> {
    let _project = project::get_by_name(&mut conn, project_name).await?;
    rule::delete(&mut conn, rule_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Returns stored variant weight overrides for a segment+feature+environment.
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
