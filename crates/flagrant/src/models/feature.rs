use std::cmp::Ordering;

use crate::errors::FlagrantError;
use std::collections::HashMap;

use flagrant_types::{
    Environment, Feature, FeatureValue, Project, TagList, Variant,
    payload::{FeaturePatch, VariantPatchOp},
};
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use smallvec::SmallVec;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteRow};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct SQLFeatures {}

pub struct FeatureUpdate<'a> {
    conn: &'a mut SqliteConnection,
    environment: &'a Environment,
    feature: &'a Feature,
    new_name: Option<String>,
    new_value: Option<FeatureValue>,
    is_enabled: Option<bool>,
}

impl<'a> FeatureUpdate<'a> {
    fn new(
        conn: &'a mut SqliteConnection,
        environment: &'a Environment,
        feature: &'a Feature,
    ) -> Self {
        Self {
            conn,
            environment,
            feature,
            new_name: None,
            new_value: None,
            is_enabled: None,
        }
    }
    pub fn name(mut self, name: String) -> Self {
        self.new_name = Some(name);
        self
    }
    pub fn value(mut self, value: FeatureValue) -> Self {
        self.new_value = Some(value);
        self
    }
    pub fn enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }
    pub async fn update(self) -> anyhow::Result<()> {
        let name = self.new_name.as_ref().unwrap_or(&self.feature.name);
        let value = self
            .new_value
            .unwrap_or_else(|| self.feature.get_default_value().clone());
        let is_enabled = self.is_enabled.unwrap_or(self.feature.is_enabled);
        let mut tx = self.conn.begin().await?;

        // In transaction, update feature properties first
        SQLFeatures::update_feature(&mut *tx, params![self.feature.id, name, is_enabled])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update a feature", e))?;

        // Then update the feature value, which is stored as the default variant
        variant::create_control(&mut tx, self.environment, self.feature, value)
            .await
            .map_err(|e| match e.downcast::<sqlx::Error>() {
                Ok(db_err) => FlagrantError::QueryFailed("Could not update a feature", db_err),
                Err(e) => FlagrantError::UnexpectedFailure("Error while updating a feature", e),
            })?;

        tx.commit().await?;
        Ok(())
    }
}

/// Creates a new feature with given `name` and `value`.
///
/// The default value is seeded as a control variant in every environment that already
/// exists in the project, so the feature is immediately usable everywhere. Each
/// environment owns its control variant independently — subsequent value changes
/// affect only the environment they are applied to.
pub async fn create(
    conn: &mut SqliteConnection,
    environment: &Environment,
    name: String,
    value: FeatureValue,
    is_enabled: bool,
    is_active: bool,
) -> anyhow::Result<Feature> {
    let mut tx = conn.begin().await?;
    let mut feature = SQLFeatures::create_feature(
        &mut *tx,
        params![environment.project_id, name, is_active, is_enabled],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not create a feature", e))?;

    let project = Project {
        id: environment.project_id,
        ..Default::default()
    };

    // Default value gets turned into a control variant for all existing environments.
    for env in &super::environment::get_by_project(&mut tx, &project).await? {
        let variant = variant::create_control(&mut tx, env, &feature, value.clone()).await?;
        if env.id == environment.id {
            feature.variants.push(variant);
        }
    }

    feature.validate()?;
    tx.commit().await?;

    Ok(feature)
}

/// Returns feature of given `feature_id` or Error if no feature was found.
pub async fn get_by_id(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<Feature> {
    let mut tx = conn.begin().await?;
    let feature = SQLFeatures::fetch_feature_by_id(&mut *tx, params![feature_id], |row| {
        row_to_feature(row, environment)
    })
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

    let variants = variant::get_for_feature(&mut tx, environment, feature.id)
        .await
        .unwrap_or_default();

    tx.commit().await?;
    Ok(feature.with_variants(variants))
}

/// Returns feature with exact `name` or Error if no feature was found.
///
/// Features names are unique therefore at most one feature is returned.
pub async fn get_by_name(
    conn: &mut SqliteConnection,
    environment: &Environment,
    name: String,
) -> anyhow::Result<Feature> {
    let feature = SQLFeatures::fetch_feature_by_name(
        &mut *conn,
        params![environment.project_id, name],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

    let variants = variant::get_for_feature(conn, environment, feature.id)
        .await
        .unwrap_or_default();

    Ok(feature.with_variants(variants))
}

/// Returns features with name starting by given `prefix`.
///
/// For performance reasons each feature is returned with its control variant only.
pub async fn get_by_prefix(
    conn: &mut SqliteConnection,
    environment: &Environment,
    prefix: String,
) -> anyhow::Result<Vec<Feature>> {
    let features = SQLFeatures::fetch_features_by_pattern(
        conn,
        params![environment.project_id, environment.id, format!("{prefix}%")],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

    Ok(features)
}

/// Returns all features for given `environment`.
///
/// For performance reasons each feature is returned with its control variant only.
pub async fn get_all(
    conn: &mut SqliteConnection,
    environment: &Environment,
    is_active: Option<bool>,
    is_enabled: Option<bool>,
    pattern: Option<String>,
    tags_included: Option<SmallVec<[&str; 3]>>,
    tags_excluded: Option<SmallVec<[&str; 3]>>,
) -> anyhow::Result<Vec<Feature>> {
    let has_included = tags_included.as_ref().map(|t| !t.is_empty());
    let has_excluded = tags_excluded.as_ref().map(|t| !t.is_empty());
    let has_pattern = pattern.is_some();

    Ok(SQLFeatures::fetch_features_for_environment(
        conn,
        |cond_id| match cond_id {
            FetchFeaturesForEnvironment::Pattern => has_pattern,
            FetchFeaturesForEnvironment::IsActive => is_active.is_some(),
            FetchFeaturesForEnvironment::IsEnabled => is_enabled.is_some(),
            FetchFeaturesForEnvironment::TagsIncluded => has_included.unwrap_or(false),
            FetchFeaturesForEnvironment::TagsExcluded => has_excluded.unwrap_or(false),
        },
        params![
            environment.project_id,
            environment.id,
            is_active,
            is_enabled,
            pattern,
            into_json_string(tags_included),
            into_json_string(tags_excluded)
        ],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of features", e))?)
}

pub fn update_one<'a>(
    conn: &'a mut SqliteConnection,
    environment: &'a Environment,
    feature: &'a Feature,
) -> FeatureUpdate<'a> {
    FeatureUpdate::new(conn, environment, feature)
}

pub async fn bump_up_accumulators(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<()> {
    SQLFeatures::update_feature_variants_accumulators(conn, params![environment.id, feature_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not bump up variants accumulators", e))?;

    Ok(())
}

/// Applies a `FeaturePatch` to the given feature atomically within a single transaction.
///
/// Operations are applied in the following order to ensure weight constraints remain
/// satisfiable throughout the transaction:
/// 1. Feature-level property changes (is_enabled, is_active)
/// 2. Variant deletes (free up weight)
/// 3. Variant updates (SetValue / SetWeight, grouped by variant id)
/// 4. Variant adds (consume weight)
pub async fn apply_patch(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    patch: FeaturePatch,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    // Feature-level properties
    if let Some(enabled) = patch.is_enabled {
        SQLFeatures::update_feature(&mut *tx, params![feature.id, &feature.name, enabled])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update feature", e))?;
    }
    if let Some(active) = patch.is_active {
        SQLFeatures::update_feature_is_active(&mut *tx, params![feature.id, active])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update feature active state", e))?;
    }
    // Partition variant ops: deletes first, then updates, then adds
    let (deletes, rest): (Vec<_>, Vec<_>) = patch
        .variants
        .into_iter()
        .partition(|op| matches!(op, VariantPatchOp::Delete { .. }));
    let (updates, adds): (Vec<_>, Vec<_>) = rest
        .into_iter()
        .partition(|op| !matches!(op, VariantPatchOp::Add { .. }));

    // Apply deletes
    for op in deletes {
        if let VariantPatchOp::Delete { id } = op {
            let var = variant::get_by_id(&mut tx, environment, id).await?;

            // Control variant cannot be deleted via PATCH operation - the only way
            // to delete it is a DELETE request to remove the entire feature.
            if !var.is_control() {
                variant::delete(&mut tx, environment, &var).await?;
            }
        }
    }

    // Group SetValue/SetWeight ops by variant id, fetch current state once, then update
    let mut update_map: HashMap<i32, (Option<FeatureValue>, Option<u8>)> = HashMap::new();
    for op in updates {
        match op {
            VariantPatchOp::SetValue { id, value } => {
                update_map.entry(id).or_default().0 = Some(value);
            }
            VariantPatchOp::SetWeight { id, weight } => {
                update_map.entry(id).or_default().1 = Some(weight);
            }
            _ => {}
        }
    }
    for (id, (new_value, new_weight)) in update_map {
        let var = variant::get_by_id(&mut tx, environment, id).await?;
        let value = new_value.unwrap_or_else(|| var.value.clone());
        let weight = new_weight.unwrap_or(var.weight);

        // The control variant cannot be modified at the variant level.
        // Its weight is auto-adjusted and its value must be updated via Feature::update_one.
        if var.is_control() {
            if new_weight.is_some() {
                return Err(FlagrantError::InvalidOperation(
                    "Setting weight on control variant is not allowed. Weight is auto-adjusted.",
                )
                .into());
            }
            update_one(&mut tx, environment, feature)
                .value(value)
                .update()
                .await?;
        } else {
            variant::update_one(&mut tx, environment, &var, value, weight).await?;
        }
    }

    // Apply adds
    for op in adds {
        if let VariantPatchOp::Add { value, weight } = op {
            variant::create(&mut tx, environment, feature, value, weight).await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Permanently deletes a feature and all of its variants within a single transaction.
///
/// Variants must be removed before the feature row itself due to foreign-key constraints.
/// Non-control variants are deleted first; the control variant is deleted last because
/// the backend rejects control-variant deletion while other variants still exist.
pub async fn delete(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let mut vars = variant::get_for_feature(&mut tx, environment, feature.id).await?;

    // Sort variants so that control ones go last in the vector.
    // This is required because of the strict deletion policy - control variants
    // cannot be deleted while other variants still exist.
    vars.sort_by(|a, _| match a.is_control() {
        true => Ordering::Greater,
        false => Ordering::Less,
    });

    // In transaction, remove all feature variants first.
    // Due to the sorting above, the control variant will be deleted last.
    for var in vars {
        variant::delete(&mut tx, environment, &var).await?;
    }

    // Then remove the feature value and the entire feature definition.
    SQLFeatures::delete_tags_for_feature(&mut *tx, params![feature.id]).await?;
    SQLFeatures::delete_variants_for_feature(&mut *tx, params![feature.id]).await?;
    SQLFeatures::delete_feature(&mut *tx, params![feature.id]).await?;

    tx.commit().await?;
    Ok(())
}

/// Transforms database result serialized as `SqliteRow` into a `Feature` model.
/// If there is a control variant detected, creates a default variant stored
/// inside feature's `variants` vector.
///
/// Default variant is what the "default" feature values is meant to be.
pub(crate) fn row_to_feature(row: SqliteRow, environment: &Environment) -> Feature {
    let mut variants = Vec::with_capacity(1);

    if let Ok(Some(variant_id)) = row.try_get("variant_id")
        && let Ok(Some(variant_value)) = row.try_get("value")
    {
        variants.push(Variant::build_default(
            environment,
            variant_id,
            variant_value,
        ))
    }

    Feature {
        id: row.get("feature_id"),
        project_id: row.get("project_id"),
        is_enabled: row.get("is_enabled"),
        is_active: row.get("is_active"),
        name: row.get("name"),
        tags: row.try_get("tags").unwrap_or(TagList(vec![])),
        variants,
    }
}

fn surround_string(s: &str, open_ch: char, close_ch: char) -> String {
    let mut buf = String::with_capacity(s.len() + 2);
    buf.push(open_ch);
    buf.push_str(s);
    buf.push(close_ch);
    buf
}

fn into_json_string(tags: Option<SmallVec<[&str; 3]>>) -> Option<String> {
    tags.map(|vt| {
        let quoted_tags: Vec<String> = vt.iter().map(|t| surround_string(t, '"', '"')).collect();
        surround_string(&quoted_tags.join(","), '[', ']')
    })
}
