use hugsqlx::{params, HugSqlx};
use sqlx::{sqlite::SqliteRow, Acquire, Pool, Row, Sqlite};

use crate::errors::DbError;
use flagrant_types::{Environment, Feature, FeatureValueType};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct Features {}

pub async fn create(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
    value: Option<(String, FeatureValueType)>,
    is_enabled: bool,
) -> anyhow::Result<Feature> {
    let mut tx = pool.begin().await?;
    let mut feature = Features::create_feature(
        &mut *tx,
        params!(environment.project_id, name, is_enabled),
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create a feature");
        DbError::QueryFailed
    })?;

    if let Some((value, value_type)) = value {
        Features::create_feature_value(
            &mut *tx,
            params![environment.id, feature.id, &value, &value_type],
        )
        .await?;

        feature.value = Some((value, value_type));
    }

    tx.commit().await?;
    Ok(feature)
}

pub async fn fetch(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<Feature> {
    let feature =
        Features::fetch_feature(pool, params![environment.id, feature_id], row_to_feature)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch a feature");
                DbError::QueryFailed
            })?;

    Ok(feature)
}

pub async fn fetch_by_name(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
) -> anyhow::Result<Feature> {
    let feature = Features::fetch_feature_by_name(
        pool,
        params![environment.id, environment.project_id, name],
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch a feature");
        DbError::QueryFailed
    })?;

    Ok(feature)
}

pub async fn fetch_by_prefix(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    prefix: String,
) -> anyhow::Result<Vec<Feature>> {
    let features = Features::fetch_features_by_pattern(
        pool,
        params![environment.id, environment.project_id, format!("{prefix}%")],
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch a feature");
        DbError::QueryFailed
    })?;

    Ok(features)
}

pub async fn update(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    new_name: String,
    new_value: Option<(String, FeatureValueType)>,
    is_enabled: bool,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    // in transaction, update feature definition first
    Features::update_feature(&mut *tx, params![feature.id, new_name, is_enabled])
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not update a feature");
            DbError::QueryFailed
        })?;

    // ...and then the feature value, if one was provided
    if let Some((value, value_type)) = new_value {
        Features::upsert_feature_value(
            &mut *tx,
            params![environment.id, feature.id, value, value_type],
        )
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not update a feature value/type");
            DbError::QueryFailed
        })?;
    }

    tx.commit().await?;
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

    // in transaction, remove all feature variants first
    for var in vars {
        variant::delete(conn, var.id).await?;
    }

    // ...and then feature value and entire feature definition
    Features::delete_feature_values(&mut *tx, params![feature.id]).await?;
    Features::delete_feature(&mut *tx, params![feature.id]).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn list(pool: &Pool<Sqlite>, environment: &Environment) -> anyhow::Result<Vec<Feature>> {
    Ok(Features::fetch_features_for_environment(
        pool,
        params![environment.id, environment.project_id],
        row_to_feature,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch features for project");
        DbError::QueryFailed
    })?)
}

/// Transforms an `SqliteRow` into a `Feature`.
/// As feature value is optional, transformation takes care of missing value as well.
fn row_to_feature(row: SqliteRow) -> Feature {
    let value: Option<String> = row.try_get("value").ok();
    let value_type: Option<FeatureValueType> = row.try_get("value_type").ok();

    Feature {
        id: row.get("feature_id"),
        project_id: row.get("project_id"),
        is_enabled: row.get("is_enabled"),
        name: row.get("name"),
        value: value.map(|v| (v, value_type.unwrap_or_default())),
    }
}
