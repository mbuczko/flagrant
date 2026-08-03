use std::collections::HashMap;

use flagrant_types::{Comparator, SegmentGroup, SegmentRule, Subject};
use hugsqlx::{HugSqlx, params};
use sqlx::SqliteConnection;

use super::segment;
use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/segments.sql"]
struct SQLSegments {}

#[derive(sqlx::FromRow)]
struct RuleRow {
    rule_id: i32,
    group_id: i32,
    #[sqlx(rename = "driver")]
    subject: Subject,
    comparator: Comparator,
    value: String,
}

/// Adds a rule to the given group, then reconciles already distributed identities against
/// the segment's updated rules. This is the shared mutation point for both the CLI's
/// `segment::patch` and the direct `POST .../rules` REST endpoint, so both trigger segment
/// reconciliation the same way.
pub async fn add(
    conn: &mut SqliteConnection,
    segment_id: i32,
    group_id: i32,
    subject: Subject,
    comparator: Comparator,
    value: String,
) -> anyhow::Result<SegmentRule> {
    let rule = SQLSegments::add_rule::<_, SegmentRule>(
        &mut *conn,
        params![group_id, subject, comparator, value],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not add rule", e))?;

    segment::reconcile_rules_changed(conn, segment_id).await?;
    Ok(rule)
}

/// Deletes a single rule by ID, then reconciles already distributed identities against the
/// segment's updated rules (see [`add`] for why this lives at the mutation point).
pub async fn delete(
    conn: &mut SqliteConnection,
    segment_id: i32,
    rule_id: i32,
) -> anyhow::Result<()> {
    SQLSegments::delete_rule::<_>(&mut *conn, params![rule_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete rule", e))?;

    segment::reconcile_rules_changed(conn, segment_id).await?;
    Ok(())
}

/// Updates a rule's value, then reconciles already distributed identities against the
/// segment's updated rules (see [`add`] for why this lives at the mutation point).
pub async fn set_value(
    conn: &mut SqliteConnection,
    segment_id: i32,
    rule_id: i32,
    value: String,
) -> anyhow::Result<()> {
    SQLSegments::update_rule_value::<_>(&mut *conn, params![rule_id, value])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update rule value", e))?;

    segment::reconcile_rules_changed(conn, segment_id).await?;
    Ok(())
}

/// Updates a rule's comparator, then reconciles already distributed identities against the
/// segment's updated rules (see [`add`] for why this lives at the mutation point).
pub async fn set_comparator(
    conn: &mut SqliteConnection,
    segment_id: i32,
    rule_id: i32,
    comparator: Comparator,
) -> anyhow::Result<()> {
    SQLSegments::update_rule_comparator::<_>(&mut *conn, params![rule_id, comparator])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update rule comparator", e))?;

    segment::reconcile_rules_changed(conn, segment_id).await?;
    Ok(())
}

/// Removes rules from each group's in-memory list - mirrors a committed `DeleteRule` op
/// without hitting the DB again.
pub(crate) fn remove_from_groups(groups: &mut [SegmentGroup], rule_id: i32) {
    for g in groups {
        g.rules.retain(|r| r.id != rule_id);
    }
}

/// Updates a rule's value in each group's in-memory list - mirrors a committed
/// `SetRuleValue` op without hitting the DB again.
pub(crate) fn update_value_in_groups(groups: &mut [SegmentGroup], rule_id: i32, value: String) {
    for g in groups {
        if let Some(r) = g.rules.iter_mut().find(|r| r.id == rule_id) {
            r.value = value;
            return;
        }
    }
}

/// Updates a rule's comparator in each group's in-memory list - mirrors a committed
/// `SetRuleComparator` op without hitting the DB again.
pub(crate) fn update_comparator_in_groups(
    groups: &mut [SegmentGroup],
    rule_id: i32,
    comparator: Comparator,
) {
    for g in groups {
        if let Some(r) = g.rules.iter_mut().find(|r| r.id == rule_id) {
            r.comparator = comparator;
            return;
        }
    }
}

fn collect_rules(rows: Vec<RuleRow>) -> HashMap<i32, Vec<SegmentRule>> {
    let mut map: HashMap<i32, Vec<SegmentRule>> = HashMap::new();
    for row in rows {
        map.entry(row.group_id).or_default().push(SegmentRule {
            id: row.rule_id,
            subject: row.subject,
            comparator: row.comparator,
            value: row.value,
        });
    }
    map
}

/// Fetches and groups rules for a single segment.
pub(crate) async fn collect_rules_for_segment(
    conn: &mut SqliteConnection,
    segment_id: i32,
) -> anyhow::Result<HashMap<i32, Vec<SegmentRule>>> {
    let rows = SQLSegments::fetch_rules_for_segment::<_, RuleRow>(conn, params![segment_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment rules", e))?;

    Ok(collect_rules(rows))
}

/// Fetches and groups all rules for a project (used for bulk segment loading).
pub(crate) async fn collect_rules_for_project(
    conn: &mut SqliteConnection,
    project_id: i32,
) -> anyhow::Result<HashMap<i32, Vec<SegmentRule>>> {
    let rows = SQLSegments::fetch_rules::<_, RuleRow>(conn, params![project_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment rules", e))?;

    Ok(collect_rules(rows))
}
