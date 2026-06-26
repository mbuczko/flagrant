use std::collections::HashMap;

use flagrant_types::{Comparator, SegmentDriver, SegmentGroup, SegmentRule};
use hugsqlx::{HugSqlx, params};
use sqlx::SqliteConnection;

use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/segments.sql"]
struct SQLSegments {}

#[derive(sqlx::FromRow)]
pub(crate) struct RuleRow {
    pub(crate) rule_id: i32,
    pub(crate) group_id: i32,
    pub(crate) driver: SegmentDriver,
    pub(crate) comparator: Comparator,
    pub(crate) value: String,
}

/// Adds a rule to the given group.
pub async fn add(
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

/// Deletes a single rule by ID.
pub async fn delete(conn: &mut SqliteConnection, rule_id: i32) -> anyhow::Result<()> {
    SQLSegments::delete_rule::<_>(conn, params![rule_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not delete rule", e))?;
    Ok(())
}

/// Removes rules from each group's in-memory list — mirrors a committed `DeleteRule` op
/// without hitting the DB again.
pub(crate) fn remove_from_groups(groups: &mut [SegmentGroup], rule_id: i32) {
    for g in groups {
        g.rules.retain(|r| r.id != rule_id);
    }
}

/// Groups `RuleRow`s by `group_id` into a map consumed by the segment loading helpers.
pub(crate) fn collect_rules(rows: Vec<RuleRow>) -> HashMap<i32, Vec<SegmentRule>> {
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
