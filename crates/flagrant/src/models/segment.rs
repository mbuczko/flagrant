use std::collections::HashMap;

use flagrant_types::{
    GroupConnector, Project, Segment, SegmentFeatureOverride, SegmentGroup, SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp, SegmentVariantWeight},
};
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use sqlx::{Acquire, SqliteConnection};

use super::{environment, rule, variant};
use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/segments.sql"]
struct SQLSegments {}

#[derive(sqlx::FromRow)]
struct SegmentRow {
    segment_id: i32,
    project_id: i32,
    name: String,
    description: Option<String>,
}

#[derive(sqlx::FromRow)]
struct GroupRow {
    group_id: i32,
    segment_id: i32,
    position: i32,
    label: String,
    connector: Option<GroupConnector>,
    description: Option<String>,
}

/// Creates a new segment in the given project and returns it with an empty groups list.
pub async fn create(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
    description: Option<String>,
) -> anyhow::Result<Segment> {
    let row = SQLSegments::create_segment::<_, SegmentRow>(
        &mut *conn,
        params![project.id, name, description],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not create segment", e))?;

    let segment = Segment {
        id: row.segment_id,
        project_id: row.project_id,
        name: row.name,
        description: row.description,
        groups: vec![],
    };
    segment.validate()?;
    Ok(segment)
}

/// Fetches a segment by its numeric ID, including all groups and rules.
pub async fn get_by_id(
    conn: &mut SqliteConnection,
    project: &Project,
    segment_id: i32,
) -> anyhow::Result<Segment> {
    let row = SQLSegments::fetch_segment_by_id::<_, SegmentRow>(
        &mut *conn,
        params![segment_id, project.id],
    )
    .await
    .map_err(|_| FlagrantError::NotFound("Segment not found"))?;

    load_segment(&mut *conn, row).await
}

/// Fetches a segment by name, including all groups and rules.
pub async fn get_by_name(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
) -> anyhow::Result<Segment> {
    let row =
        SQLSegments::fetch_segment_by_name::<_, SegmentRow>(&mut *conn, params![name, project.id])
            .await
            .map_err(|_| FlagrantError::NotFound("Segment not found"))?;

    load_segment(&mut *conn, row).await
}

/// Lists all segments for the project, optionally filtered by a name pattern.
pub async fn get_all(
    conn: &mut SqliteConnection,
    project: &Project,
    pattern: Option<String>,
) -> anyhow::Result<Vec<Segment>> {
    let rows = match pattern {
        Some(pat) => {
            SQLSegments::fetch_segments_by_pattern::<_, SegmentRow>(
                &mut *conn,
                params![project.id, pat],
            )
            .await
        }
        None => SQLSegments::fetch_segments::<_, SegmentRow>(&mut *conn, params![project.id]).await,
    }
    .map_err(|e| FlagrantError::QueryFailed("Could not list segments", e))?;

    load_all_segments(&mut *conn, rows).await
}

/// Updates the name and description of an existing segment.
pub async fn update(
    conn: &mut SqliteConnection,
    segment: &Segment,
    name: &str,
    description: Option<&str>,
) -> anyhow::Result<()> {
    SQLSegments::update_segment::<_>(&mut *conn, params![segment.id, name, description])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update segment", e))?;

    Ok(())
}

/// Returns the stored variant weight overrides for a given segment + feature + environment.
pub async fn get_variant_weights(
    conn: &mut SqliteConnection,
    segment_id: i32,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Vec<(i32, u8)>> {
    variant::get_segment_weights(conn, segment_id, feature_id, environment_id).await
}

/// Returns per-segment weight overrides for the given feature + environment.
///
/// Each entry is `(segment_name, weights)`. The query returns one row per
/// (segment, variant); this function groups them by segment name.
pub async fn list_overrides_for_feature(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
) -> anyhow::Result<Vec<(String, Vec<SegmentVariantWeight>)>> {
    let rows =
        variant::get_segment_overrides_with_weights(conn, feature_id, environment_id).await?;

    let mut result: Vec<(String, Vec<SegmentVariantWeight>)> = Vec::new();
    for (name, variant_id, weight) in rows {
        if let Some(entry) = result.iter_mut().find(|(n, _)| n == &name) {
            entry.1.push(SegmentVariantWeight { variant_id, weight });
        } else {
            result.push((name, vec![SegmentVariantWeight { variant_id, weight }]));
        }
    }
    Ok(result)
}

/// Returns every feature this segment overrides within the given environment, each with
/// its full weight breakdown (including the control variant's auto-balanced remainder).
pub async fn list_overridden_features(
    conn: &mut SqliteConnection,
    environment_id: i32,
    segment_id: i32,
) -> anyhow::Result<Vec<SegmentFeatureOverride>> {
    let rows = variant::get_features_overridden_by_segment(conn, segment_id, environment_id).await?;

    let mut result: Vec<SegmentFeatureOverride> = Vec::new();
    for (feature_id, feature_name, ov) in rows {
        if let Some(entry) = result.iter_mut().find(|f| f.feature_id == feature_id) {
            entry.weights.push(ov);
        } else {
            result.push(SegmentFeatureOverride {
                feature_id,
                feature_name,
                weights: vec![ov],
            });
        }
    }
    Ok(result)
}

/// Deletes a segment and all its associated groups and rules.
pub async fn delete(conn: &mut SqliteConnection, segment: &Segment) -> anyhow::Result<()> {
    SQLSegments::delete_segment::<_>(&mut *conn, params![segment.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete segment", e))?;

    Ok(())
}

// Group operations

/// Adds a new group to a segment.
///
/// The label is computed as `group-{MAX(N)+1}` over all existing labels to ensure
/// stable, non-reused identifiers. The first group always has no connector; subsequent
/// groups default to `AND` if no connector is specified.
pub async fn add_group(
    conn: &mut SqliteConnection,
    segment: &Segment,
    description: Option<String>,
    connector: Option<GroupConnector>,
) -> anyhow::Result<SegmentGroup> {
    // Load existing groups to determine next position and stable label number.
    let existing =
        SQLSegments::fetch_groups_for_segment::<_, GroupRow>(&mut *conn, params![segment.id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not fetch groups", e))?;

    let next_position = existing.iter().map(|g| g.position).max().unwrap_or(-1) + 1;

    // Labels are never reused — pick MAX(N) + 1 across all existing labels.
    let max_label_num = existing
        .iter()
        .filter_map(|g| g.label.strip_prefix("group-"))
        .filter_map(|n| n.parse::<i32>().ok())
        .max()
        .unwrap_or(0);

    let label = format!("group-{}", max_label_num + 1);

    // First group always has no connector; subsequent groups default to AND if unspecified.
    let effective_connector: Option<GroupConnector> = if existing.is_empty() {
        None
    } else {
        Some(connector.unwrap_or(GroupConnector::And))
    };

    let row = SQLSegments::add_group::<_, GroupRow>(
        &mut *conn,
        params![
            segment.id,
            next_position,
            label,
            effective_connector,
            description
        ],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not add group", e))?;

    Ok(SegmentGroup {
        id: row.group_id,
        label: row.label,
        description: row.description,
        connector: row.connector,
        rules: vec![],
    })
}

/// Deletes a group and all its rules, then clears the connector on the new head group
/// so the remaining first group is never left with an AND/AND NOT connector.
pub async fn delete_group(
    conn: &mut SqliteConnection,
    segment: &Segment,
    group_id: i32,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    SQLSegments::delete_group::<_>(&mut *tx, params![group_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete group", e))?;

    // Ensure the new head has no AND/AND NOT connector.
    SQLSegments::clear_initial_group_connector::<_>(&mut *tx, params![segment.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not clear head connector", e))?;

    tx.commit().await?;
    Ok(())
}

/// Applies a batch of staged operations to a segment and returns the updated segment.
///
/// Each op is executed immediately against the DB; the in-memory `segment` is kept in
/// sync so that subsequent ops in the same batch (e.g. `AddRule` after `AddGroup`) can
/// resolve labels and IDs without an extra round-trip.
pub async fn patch(
    conn: &mut SqliteConnection,
    project: &Project,
    mut segment: Segment,
    patch: SegmentPatch,
) -> anyhow::Result<Segment> {
    for op in patch.ops {
        match op {
            SegmentPatchOp::SetName(name) => {
                segment.name = name;
                segment.validate()?;
                update(
                    conn,
                    &segment,
                    &segment.name,
                    segment.description.as_deref(),
                )
                .await?;
            }
            SegmentPatchOp::SetDescription(description) => {
                update(conn, &segment, &segment.name, description.as_deref()).await?;
                segment.description = description;
            }
            SegmentPatchOp::AddGroup {
                connector,
                description,
            } => {
                let group = add_group(conn, &segment, description, connector).await?;
                segment.groups.push(group);
            }
            SegmentPatchOp::DeleteGroup { label } => {
                let group_id = segment
                    .groups
                    .iter()
                    .find(|g| g.label == label)
                    .map(|g| g.id)
                    .ok_or_else(|| FlagrantError::NotFound("Group not found"))?;

                delete_group(conn, &segment, group_id).await?;
                segment.groups.retain(|g| g.label != label);

                if let Some(head) = segment.groups.first_mut() {
                    head.connector = None;
                }
            }
            SegmentPatchOp::AddRule {
                group_label,
                driver,
                comparator,
                value,
            } => {
                let group_id = segment
                    .groups
                    .iter()
                    .find(|g| g.label == group_label)
                    .map(|g| g.id)
                    .ok_or_else(|| FlagrantError::NotFound("Group not found"))?;
                let sr = rule::add(conn, group_id, driver, comparator, value).await?;

                if let Some(g) = segment.groups.iter_mut().find(|g| g.label == group_label) {
                    g.rules.push(sr);
                }
            }
            SegmentPatchOp::DeleteRule { rule_id } => {
                rule::delete(conn, rule_id).await?;
                rule::remove_from_groups(&mut segment.groups, rule_id);
            }
            SegmentPatchOp::SetFeatureOverride {
                feature_id,
                environment_id,
                variant_weights,
            } => {
                let environment = environment::get_by_id(&mut *conn, environment_id).await?;

                variant::delete_segment_weights_for_feature(
                    &mut *conn,
                    segment.id,
                    feature_id,
                    environment_id,
                )
                .await?;
                for vw in &variant_weights {
                    variant::set_segment_weight(
                        &mut *conn,
                        &environment,
                        segment.id,
                        vw.variant_id,
                        vw.weight,
                    )
                    .await?;
                }
                // Balance the control variant's remainder within this segment, mirroring
                // how organic weights always sum to 100.
                variant::balance_segment_control_weight(
                    &mut *conn,
                    &environment,
                    segment.id,
                    feature_id,
                )
                .await?;
            }
            SegmentPatchOp::UnsetFeatureOverride {
                feature_id,
                environment_id,
            } => {
                variant::delete_segment_weights_for_feature(
                    &mut *conn,
                    segment.id,
                    feature_id,
                    environment_id,
                )
                .await?;
            }
        }
    }

    get_by_id(conn, project, segment.id).await
}

//
// Helpers to load and construct segments
//

/// Groups rules by group_id, then groups `SegmentGroup`s by segment_id,
/// consuming the rules map built by [`collect_rules`].
fn collect_groups(
    rows: Vec<GroupRow>,
    mut rules: HashMap<i32, Vec<SegmentRule>>,
) -> HashMap<i32, Vec<SegmentGroup>> {
    let mut map: HashMap<i32, Vec<SegmentGroup>> = HashMap::new();
    for row in rows {
        let group_rules = rules.remove(&row.group_id).unwrap_or_default();
        map.entry(row.segment_id).or_default().push(SegmentGroup {
            id: row.group_id,
            label: row.label,
            description: row.description,
            connector: row.connector,
            rules: group_rules,
        });
    }
    map
}

/// Loads groups and rules for a single segment
async fn load_segment(conn: &mut SqliteConnection, row: SegmentRow) -> anyhow::Result<Segment> {
    let group_rows =
        SQLSegments::fetch_groups_for_segment::<_, GroupRow>(&mut *conn, params![row.segment_id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment groups", e))?;

    let mut rules = rule::collect_rules_for_segment(&mut *conn, row.segment_id).await?;
    let groups = group_rows
        .into_iter()
        .map(|g| {
            let group_rules = rules.remove(&g.group_id).unwrap_or_default();
            SegmentGroup {
                id: g.group_id,
                label: g.label,
                description: g.description,
                connector: g.connector,
                rules: group_rules,
            }
        })
        .collect();

    Ok(Segment {
        id: row.segment_id,
        project_id: row.project_id,
        description: row.description,
        name: row.name,
        groups,
    })
}

/// Loads groups and rules for multiple segments using two project-scoped bulk queries,
/// then assembles the nested `Segment` structs via in-memory HashMaps.
/// Assuming all rows belong to the same project.
async fn load_all_segments(
    conn: &mut SqliteConnection,
    rows: Vec<SegmentRow>,
) -> anyhow::Result<Vec<Segment>> {
    if rows.is_empty() {
        return Ok(vec![]);
    }

    let project_id = rows[0].project_id;
    let group_rows =
        SQLSegments::fetch_groups_for_segments::<_, GroupRow>(&mut *conn, params![project_id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment groups", e))?;

    let rules = rule::collect_rules_for_project(&mut *conn, project_id).await?;
    let mut groups = collect_groups(group_rows, rules);

    Ok(rows
        .into_iter()
        .map(|row| {
            let seg_groups = groups.remove(&row.segment_id).unwrap_or_default();
            Segment {
                id: row.segment_id,
                project_id: row.project_id,
                name: row.name,
                description: row.description,
                groups: seg_groups,
            }
        })
        .collect())
}
