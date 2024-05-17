use hugsqlx::{params, HugSqlx};
use sqlx::{sqlite::SqliteRow, Acquire, Pool, Row, Sqlite, SqliteConnection};
use serde_valid::Validate;
use crate::errors::DbError;
use flagrant_types::{Environment, Feature, FeatureValue, Variant};

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
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create a feature");
        DbError::QueryFailed
    })?;

    if let Some(FeatureValue(value, _)) = value {
        // if default value was provided, turn it into a control variant.
        // note, value type is stored at feature level, variants hold purely
        // stringified values only which should eventually conform to that type.

        let variant = variant::upsert_default(&mut tx, environment, &feature, value).await?;
        feature.set_default_variant(variant);
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
    let feature = Features::fetch_feature(pool, params![feature_id], row_to_feature)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not fetch a feature");
            DbError::QueryFailed
        })?;

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
    let feature = Features::fetch_feature_by_name(
        pool,
        params![environment.project_id, name],
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch a feature");
        DbError::QueryFailed
    })?;

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
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch a feature");
        DbError::QueryFailed
    })?;

    Ok(features)
}

/// Returns all features for given `environment`.
/// For performance reasons each feature is returned with its default variant only.
pub async fn list(pool: &Pool<Sqlite>, environment: &Environment) -> anyhow::Result<Vec<Feature>> {
    Ok(Features::fetch_features_for_environment(
        pool,
        params![environment.project_id, environment.id],
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch features for project");
        DbError::QueryFailed
    })?)
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
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not update a feature");
        DbError::QueryFailed
    })?;

    // ...and then the feature value which is stored as default variant
    if let Some(FeatureValue(value, _)) = new_value {
        variant::upsert_default(&mut tx, environment, feature, value)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not update a feature value/type");
                DbError::QueryFailed
            })?;
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
        tracing::error!(error = ?e, "Could not bump up feature variants accumulators");
        DbError::QueryFailed
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
    let vars = variant::list(pool, environment, feature).await?;

    // in transaction, remove all feature variants first.
    // remove default variant (which is first in vec) as last one.
    for var in vars.into_iter().rev() {
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
pub(crate) fn row_to_feature(row: SqliteRow) -> Feature {
    let mut variants = Vec::with_capacity(1);

    if let Ok(Some(variant_id)) = row.try_get("variant_id") {
        if let Ok(Some(variant_value)) = row.try_get("value") {
            variants.push(Variant::build_default(variant_id, variant_value))
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
