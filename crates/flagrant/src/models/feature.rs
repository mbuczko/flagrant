use crate::errors::FlagrantError;
use std::collections::HashMap;

use chrono::Utc;
use flagrant_types::{
    Environment, Feature, Project, TagList, Variant, VariantValue,
    payload::{FeaturePatch, RolloutPatchOp, TagPatchOp, VariantPatchOp},
};
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use smallvec::SmallVec;
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteRow};

use super::{into_json_string, rollout, variant};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct SQLFeatures {}

/// Creates a new feature with given `name` and `value`.
///
/// The default value is seeded as a control variant in every environment that already
/// exists in the project, so the feature is immediately usable everywhere. Each
/// environment owns its control variant independently - subsequent value changes
/// affect only the environment they are applied to.
pub async fn create(
    conn: &mut SqliteConnection,
    environment: &Environment,
    name: String,
    description: Option<String>,
    value: VariantValue,
    is_enabled: bool,
    is_srv: bool,
) -> anyhow::Result<Feature> {
    let mut tx = conn.begin().await?;
    let mut feature = SQLFeatures::create_feature(
        &mut *tx,
        params![
            environment.project_id,
            name,
            description,
            is_enabled,
            is_srv
        ],
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
        let variant = variant::create_control(&mut tx, env, feature.id, value.clone()).await?;
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

    let variants = variant::get_for_feature(&mut tx, environment, feature.id, None)
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
    name: &str,
) -> anyhow::Result<Feature> {
    let feature = SQLFeatures::fetch_feature_by_name(
        &mut *conn,
        params![environment.project_id, name],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

    let variants = variant::get_for_feature(conn, environment, feature.id, None)
        .await
        .unwrap_or_default();

    Ok(feature.with_variants(variants))
}

/// Returns all features for given `environment`, each with all its variants.
pub async fn get_all(
    conn: &mut SqliteConnection,
    environment: &Environment,
    is_archived: Option<bool>,
    is_enabled: Option<bool>,
    pattern: Option<String>,
    tags_included: Option<SmallVec<[&str; 3]>>,
    tags_excluded: Option<SmallVec<[&str; 3]>>,
) -> anyhow::Result<Vec<Feature>> {
    let has_included = tags_included.as_ref().map(|t| !t.is_empty());
    let has_excluded = tags_excluded.as_ref().map(|t| !t.is_empty());
    let has_pattern = pattern.is_some();

    // One row per (feature, variant) - aggregate into features below.
    let rows = SQLFeatures::fetch_features_for_environment(
        conn,
        |cond_id| match cond_id {
            FetchFeaturesForEnvironment::Pattern => has_pattern,
            FetchFeaturesForEnvironment::IsArchived => is_archived.is_some(),
            FetchFeaturesForEnvironment::IsEnabled => is_enabled.is_some(),
            FetchFeaturesForEnvironment::TagsIncluded => has_included.unwrap_or(false),
            FetchFeaturesForEnvironment::TagsExcluded => has_excluded.unwrap_or(false),
        },
        params![
            environment.project_id,
            environment.id,
            is_archived,
            is_enabled,
            pattern,
            into_json_string(tags_included),
            into_json_string(tags_excluded)
        ],
        |row| {
            let variant = if let Ok(Some(variant_id)) = row.try_get::<Option<i32>, _>("variant_id")
            {
                Some(Variant {
                    id: variant_id,
                    value: row.get("value"),
                    weight: row.get("weight"),
                    accumulator: row.try_get("accumulator").unwrap_or(0),
                    environment_id: row.try_get("environment_id").ok().flatten(),
                })
            } else {
                None
            };
            (row_to_feature(row, environment), variant)
        },
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of features", e))?;

    let mut result: Vec<Feature> = Vec::new();
    let mut id_to_idx: HashMap<i32, usize> = HashMap::new();

    for (feature, variant) in rows {
        if let Some(&idx) = id_to_idx.get(&feature.id) {
            if let Some(v) = variant {
                result[idx].variants.push(v);
            }
        } else {
            id_to_idx.insert(feature.id, result.len());
            result.push(feature.with_variants(variant.into_iter().collect()));
        }
    }
    Ok(result)
}

pub async fn bump_up_accumulators(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    segment_id: Option<i32>,
) -> anyhow::Result<()> {
    SQLFeatures::update_feature_variants_accumulators(
        conn,
        params![environment.id, feature_id, segment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not bump up variants accumulators", e))?;

    Ok(())
}

/// Applies a `FeaturePatch` to the given feature atomically within a single transaction.
///
/// Operations are applied in the following order to ensure weight constraints remain
/// satisfiable throughout the transaction:
/// 1. Feature-level property changes (is_enabled, archived_at)
/// 2. Variant deletes (free up weight)
/// 3. Variant updates (SetValue / SetWeight, grouped by variant id)
/// 4. Variant adds (consume weight)
pub async fn patch(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    patch: FeaturePatch,
) -> anyhow::Result<Option<Feature>> {
    if patch.delete {
        delete(conn, environment, feature).await?;
        return Ok(None);
    }

    patch.validate()?;

    let mut tx = conn.begin().await?;

    // Feature-level properties
    if patch.name.is_some() || patch.is_enabled.is_some() || patch.is_srv.is_some() {
        let name = patch.name.as_deref().unwrap_or(&feature.name);
        let enabled = patch.is_enabled.unwrap_or(feature.is_enabled);
        let srv = patch.is_srv.unwrap_or(feature.is_srv);

        SQLFeatures::update_feature(&mut *tx, params![feature.id, name, enabled, srv])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update feature", e))?;
    }
    if let Some(description) = patch.description {
        SQLFeatures::update_feature_description(&mut *tx, params![feature.id, description])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update feature description", e))?;
    }
    if let Some(archived) = patch.is_archived {
        let ts = if archived { Some(Utc::now()) } else { None };
        SQLFeatures::archive_feature(&mut *tx, params![feature.id, ts])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update feature active state", e))?;
    }

    // Keep only the last op per tag name, so a caller sending redundant or conflicting
    // ops for the same tag (e.g. two `Add`s, or an `Add` and a `Remove`) doesn't run
    // more queries than necessary or apply them in a surprising order.
    let mut deduped_tags: Vec<TagPatchOp> = Vec::with_capacity(patch.tags.len());

    for op in patch.tags {
        let name = match &op {
            TagPatchOp::Add(tag) | TagPatchOp::Remove(tag) => tag,
        };
        match deduped_tags.iter_mut().find(
            |existing| matches!(existing, TagPatchOp::Add(t) | TagPatchOp::Remove(t) if t == name),
        ) {
            Some(existing) => *existing = op,
            None => deduped_tags.push(op),
        }
    }
    for op in deduped_tags {
        match op {
            TagPatchOp::Add(tag) => {
                SQLFeatures::insert_tag_for_feature(&mut *tx, params![feature.id, tag])
                    .await
                    .map_err(|e| FlagrantError::QueryFailed("Could not add feature tag", e))?;
            }
            TagPatchOp::Remove(tag) => {
                SQLFeatures::delete_tag_for_feature(&mut *tx, params![feature.id, tag])
                    .await
                    .map_err(|e| FlagrantError::QueryFailed("Could not remove feature tag", e))?;
            }
        }
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
            let var = variant::get_by_id(&mut tx, environment, id, None).await?;

            // Control variant cannot be deleted via PATCH operation - the only way
            // to delete it is a DELETE request to remove the entire feature.
            if !var.is_control() {
                variant::delete(&mut tx, environment, &var).await?;
            }
        }
    }

    // Group SetValue/SetWeight ops by variant id, fetch current state once, then update
    let mut update_map: HashMap<i32, (Option<VariantValue>, Option<u8>)> = HashMap::new();

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
        let var = variant::get_by_id(&mut tx, environment, id, None).await?;

        // Routes transparently to the right underlying update for control vs. non-control
        // (see `VariantUpdate::update`) - rejects outright if `new_weight` was given for
        // the control variant, whose weight is always the auto-computed remainder.
        let mut builder = variant::update(&mut tx, environment, feature.id, &var);

        if let Some(value) = new_value {
            builder = builder.value(value);
        }
        if let Some(weight) = new_weight {
            builder = builder.weight(weight);
        }
        builder.update().await?;
    }

    // Apply adds
    for op in adds {
        if let VariantPatchOp::Add { value, weight } = op {
            variant::create(&mut tx, environment, feature, value, weight).await?;
        }
    }

    // Progressive rollout. Checked against the *post*-patch variant count (deletes/
    // updates/adds above have already settled), so this also rejects e.g. adding a 2nd
    // variant to an already-progressive feature, not just enabling on a bad count.
    let effective_rollout = match &patch.rollout {
        Some(RolloutPatchOp::Set(cfg)) => Some(cfg.clone()),
        Some(RolloutPatchOp::Unset) => None,
        None => feature.rollout.clone(),
    };

    if let Some(cfg) = &effective_rollout {
        if let Some(RolloutPatchOp::Set(_)) = &patch.rollout {
            cfg.validate_steps().map_err(FlagrantError::BadRequest)?;
        }

        let live = variant::get_for_feature(&mut tx, environment, feature.id, None).await?;
        let non_control: Vec<_> = live.iter().filter(|v| !v.is_control()).collect();
        if non_control.len() != 1 {
            return Err(FlagrantError::BadRequest(
                "Progressive rollout requires exactly one non-control variant",
            )
            .into());
        }

        if let Some(RolloutPatchOp::Set(new_cfg)) = &patch.rollout {
            let rollout_json = serde_json::to_string(new_cfg)?;
            SQLFeatures::update_feature_rollout(&mut *tx, params![feature.id, rollout_json])
                .await
                .map_err(|e| FlagrantError::QueryFailed("Could not set progressive rollout", e))?;

            variant::set_weight_and_rebalance(
                &mut tx,
                environment,
                feature.id,
                non_control[0],
                new_cfg.steps[0].weight,
            )
            .await?;

            rollout::activate(&mut tx, environment.id, feature.id).await?;
        }
    }

    if let Some(RolloutPatchOp::Unset) = &patch.rollout {
        SQLFeatures::clear_feature_rollout(&mut *tx, params![feature.id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not clear progressive rollout", e))?;
        rollout::deactivate(&mut tx, feature.id).await?;
    }

    tx.commit().await?;
    get_by_id(conn, environment, feature.id).await.map(Some)
}

/// Permanently deletes a feature and all of its variants within a single transaction.
///
/// Variants must be removed before the feature row itself due to foreign-key constraints.
/// Non-control variants are deleted first; the control variant is deleted last because
/// the backend rejects control-variant deletion while other variants still exist.
pub async fn delete(
    conn: &mut SqliteConnection,
    _environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    // Delete across all environments in dependency order to satisfy FK constraints.
    // Feature creation seeds control variants for every environment in the project, so
    // a single-environment variant loop would miss control variants from other environments
    // and leave their variant_weights rows behind, causing FK failures.
    SQLFeatures::delete_identity_variants_for_feature(&mut *tx, params![feature.id]).await?;
    SQLFeatures::delete_variant_weights_for_feature(&mut *tx, params![feature.id]).await?;
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
        name: row.get("name"),
        description: row.get("description"),
        is_enabled: row.get("is_enabled"),
        is_srv: row.get("is_srv"),
        is_archived: row
            .try_get::<Option<String>, _>("archived_at")
            .is_ok_and(|v| v.is_some()),
        tags: row.try_get("tags").unwrap_or(TagList(vec![])),
        variants,
        rollout: row
            .try_get::<Option<String>, _>("rollout")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok()),
    }
}
