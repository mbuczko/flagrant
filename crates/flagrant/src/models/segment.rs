use std::collections::HashMap;

use flagrant_types::{
    Comparator, GroupConnector, Project, Segment, SegmentDriver, SegmentGroup, SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp},
};
use hugsqlx::{HugSqlx, params};
use sqlx::{Acquire, SqliteConnection};

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

#[derive(sqlx::FromRow)]
struct RuleRow {
    rule_id: i32,
    group_id: i32,
    driver: SegmentDriver,
    comparator: Comparator,
    value: String,
}

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

    Ok(Segment {
        id: row.segment_id,
        project_id: row.project_id,
        name: row.name,
        description: row.description,
        groups: vec![],
    })
}

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

pub async fn delete(conn: &mut SqliteConnection, segment: &Segment) -> anyhow::Result<()> {
    SQLSegments::delete_segment::<_>(&mut *conn, params![segment.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete segment", e))?;

    Ok(())
}

// Group operations

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

// Rule operations

pub async fn add_rule(
    conn: &mut SqliteConnection,
    group_id: i32,
    driver: SegmentDriver,
    comparator: Comparator,
    value: String,
) -> anyhow::Result<SegmentRule> {
    Ok(
        SQLSegments::add_rule::<_, SegmentRule>(conn, params![group_id, driver, comparator, value])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not add rule", e))?,
    )
}

pub async fn patch(
    conn: &mut SqliteConnection,
    project: &Project,
    mut segment: Segment,
    patch: SegmentPatch,
) -> anyhow::Result<Segment> {
    for op in patch.ops {
        match op {
            SegmentPatchOp::SetName(name) => {
                update(conn, &segment, &name, segment.description.as_deref()).await?;
                segment.name = name;
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
                let rule = add_rule(conn, group_id, driver, comparator, value).await?;
                if let Some(g) = segment.groups.iter_mut().find(|g| g.label == group_label) {
                    g.rules.push(rule);
                }
            }
            SegmentPatchOp::DeleteRule { rule_id } => {
                delete_rule(conn, rule_id).await?;
                for g in &mut segment.groups {
                    g.rules.retain(|r| r.id != rule_id);
                }
            }
        }
    }

    get_by_id(conn, project, segment.id).await
}

pub async fn delete_rule(conn: &mut SqliteConnection, rule_id: i32) -> anyhow::Result<()> {
    SQLSegments::delete_rule::<_>(conn, params![rule_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete rule", e))?;
    Ok(())
}

fn collect_rules(rows: Vec<RuleRow>) -> HashMap<i32, Vec<SegmentRule>> {
    let mut map: HashMap<i32, Vec<SegmentRule>> = HashMap::new();
    for row in rows {
        map.entry(row.group_id).or_default().push(SegmentRule {
            id: row.rule_id,
            driver: row.driver,
            comparator: row.comparator,
            value: row.value,
        });
    }
    map
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

    let rule_rows =
        SQLSegments::fetch_rules_for_segment::<_, RuleRow>(&mut *conn, params![row.segment_id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment rules", e))?;

    let mut rules = collect_rules(rule_rows);
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
        name: row.name,
        description: row.description,
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

    let rule_rows = SQLSegments::fetch_rules::<_, RuleRow>(&mut *conn, params![project_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment rules", e))?;

    let rules = collect_rules(rule_rows);
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
