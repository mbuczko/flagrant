use std::cmp::Ordering;

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Feature, FeatureValue, Variant};
use hugsqlx::{params, HugSqlx};
use serde_valid::Validate;
use sqlx::{sqlite::SqliteRow, Acquire, Pool, Row, Sqlite, SqliteConnection};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct Features {}

/// Creates a new on/off feature with given `name` and optional `value`.
///
/// Feature value is stored as default variant and is environment-specific
/// which means it may differ across all other environments.
pub async fn create(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
    value: Option<FeatureValue>,
    is_enabled: bool,
) -> anyhow::Result<Feature> {
    let mut tx = pool.begin().await?;
    let mut feature = Features::create_feature(
        &mut *tx,
        params![
            environment.project_id,
            name,
            is_enabled,
            value
                .as_ref()
                .map(|FeatureValue(_, value_type)| value_type.clone())
                .unwrap_or_default()
        ],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not create a feature", e.to_string()))?;

    // if default value was provided, turn it into a control variant.
    if let Some(FeatureValue(value, _)) = value {
        set_default_value(&mut tx, environment, &mut feature, value).await?;
    }

    feature.validate()?;
    tx.commit().await?;

    Ok(feature)
}

/// Sets default feature value for given environment.
/// Value type is stored at feature level, however value itself is stored as a String
/// in variant. This is to enforce same type for all the variant values.
pub async fn set_default_value(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &mut Feature,
    value: String,
) -> anyhow::Result<()> {
    let variant = variant::upsert_default(conn, environment, feature, value).await?;
    feature.variants.insert(0, variant);
    Ok(())
}

/// Returns feature of given `feature_id` or Error if no feature was found.
pub async fn fetch(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<Feature> {
    let feature = Features::fetch_feature(pool, params![feature_id], |row| {
        row_to_feature(row, environment)
    })
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e.to_string()))?;

    let variants = variant::list(pool, environment, &feature)
        .await
        .unwrap_or_default();

    Ok(feature.with_variants(variants))
}

/// Returns feature with exact `name` or Error if no feature was found.
/// Features names are unique therefore at most one feature is returned.
pub async fn fetch_by_name(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
) -> anyhow::Result<Feature> {
    let feature =
        Features::fetch_feature_by_name(pool, params![environment.project_id, name], |row| {
            row_to_feature(row, environment)
        })
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e.to_string()))?;

    let variants = variant::list(pool, environment, &feature)
        .await
        .unwrap_or_default();

    Ok(feature.with_variants(variants))
}

/// Returns features with name starting by given `prefix`.
/// For performance reasons each feature is returned with its default variant only.
pub async fn fetch_by_prefix(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    prefix: String,
) -> anyhow::Result<Vec<Feature>> {
    let features = Features::fetch_features_by_pattern(
        pool,
        params![environment.project_id, environment.id, format!("{prefix}%")],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e.to_string()))?;

    Ok(features)
}

/// Returns all features for given `environment`.
/// For performance reasons each feature is returned with its default variant only.
pub async fn list(pool: &Pool<Sqlite>, environment: &Environment) -> anyhow::Result<Vec<Feature>> {
    Ok(Features::fetch_features_for_environment(
        pool,
        params![environment.project_id, environment.id],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of features", e.to_string()))?)
}

pub async fn update(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    new_name: String,
    new_value: Option<FeatureValue>,
    is_enabled: bool,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let new_value_type = new_value
        .as_ref()
        .map(|FeatureValue(_, t)| t)
        .unwrap_or_else(|| &feature.value_type);

    // in transaction, update feature properties first
    Features::update_feature(
        &mut *tx,
        params![feature.id, new_name, new_value_type, is_enabled],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not update a feature", e.to_string()))?;

    // ...and then the feature value which is stored as default variant
    if let Some(FeatureValue(value, _)) = new_value {
        variant::upsert_default(&mut tx, environment, feature, value)
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update a feature", e.to_string()))?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn bump_up_accumulators(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    by_value: i16,
) -> anyhow::Result<()> {
    Features::update_feature_variants_accumulators(
        conn,
        params![environment.id, feature.id, by_value],
    )
    .await
    .map_err(|e| {
        FlagrantError::QueryFailed("Could not bump up variants accumulators", e.to_string())
    })?;

    Ok(())
}

pub async fn delete(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let conn = tx.acquire().await?;
    let mut vars = variant::list(pool, environment, feature).await?;

    // sort variants in a way that control values are the last ones in a vector.
    // this is necessary, as deleting a control variant where the other variants
    // still exist is forbidded.

    vars.sort_by(|a, _| match a.is_control() {
        true => Ordering::Greater,
        false => Ordering::Less,
    });

    // in transaction, remove all feature variants first.
    // remove default variant (which is first in vec) as last one.
    for var in vars {
        variant::delete(conn, environment, &var).await?;
    }

    // ...and then feature value and entire feature definition
    Features::delete_variants_for_feature(&mut *tx, params![feature.id]).await?;
    Features::delete_feature(&mut *tx, params![feature.id]).await?;

    tx.commit().await?;
    Ok(())
}

/// Transforms database result serialized as `SqliteRow` into a `Feature` model.
/// If there is a control variant detected, creates a default variant accordinly
/// stored within feature's `variants` vector.
pub(crate) fn row_to_feature(row: SqliteRow, environment: &Environment) -> Feature {
    let mut variants = Vec::with_capacity(1);

    if let Ok(Some(variant_id)) = row.try_get("variant_id") {
        if let Ok(Some(variant_value)) = row.try_get("value") {
            variants.push(Variant::build_default(
                environment,
                variant_id,
                variant_value,
            ))
        }
    }
    Feature {
        id: row.get("feature_id"),
        project_id: row.get("project_id"),
        is_enabled: row.get("is_enabled"),
        name: row.get("name"),
        value_type: row.get("value_type"),
        variants,
    }
}
