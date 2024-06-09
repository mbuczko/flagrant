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
            is_enabled
        ],
        |row| row_to_feature(row, environment),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not create a feature", e))?;

    // if default value was provided, turn it into a control variant.
    if let Some(value) = value {
        let variant = variant::upsert_default(&mut tx, environment, &feature, value).await?;
        feature.variants.push(variant);
    }

    feature.validate()?;
    tx.commit().await?;

    Ok(feature)
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
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

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
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

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
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch a feature", e))?;

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
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of features", e))?)
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

    // in transaction, update feature properties first
    Features::update_feature(
        &mut *tx,
        params![feature.id, new_name, is_enabled],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not update a feature", e))?;

    // ...and then the feature value which is stored as default variant
    if let Some(value) = new_value {
        variant::upsert_default(&mut tx, environment, feature, value)
            .await
            .map_err(|e| match e.downcast::<sqlx::Error>() {
                Ok(db_err) => FlagrantError::QueryFailed("Could not update a feature", db_err),
                Err(e) => FlagrantError::UnexpectedFailure("Error while updating a feature", e),
            })?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn bump_up_accumulators(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature
) -> anyhow::Result<()> {
    Features::update_feature_variants_accumulators(
        conn,
        params![environment.id, feature.id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not bump up variants accumulators", e))?;

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

    // sort variants so, that control ones go last in a vector.
    // this is required because of the strict deletion policy - control variants
    // cannot be deleted when the other variants still exist.
    vars.sort_by(|a, _| match a.is_control() {
        true => Ordering::Greater,
        false => Ordering::Less,
    });

    // in transaction, remove all feature variants first.
    // because of the sorting done before, control variant will be deleted last.
    for var in vars {
        variant::delete(conn, environment, &var).await?;
    }

    // ...and then remove feature value and entire feature definition
    Features::delete_variants_for_feature(&mut *tx, params![feature.id]).await?;
    Features::delete_feature(&mut *tx, params![feature.id]).await?;

    tx.commit().await?;
    Ok(())
}

/// Transforms database result serialized as `SqliteRow` into a `Feature` model.
/// If there is a control variant detected, creates a default variant stored
/// stores inside feature's `variants` vector.
///
/// Default variant is what the "default" feature values is meant to be.
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
        variants,
    }
}
