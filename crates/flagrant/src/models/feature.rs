use hugsqlx::{params, HugSqlx};
use sqlx::{Pool, Row, Sqlite};

use crate::errors::DbError;
use flagrant_types::{Environment, Feature, FeatureValueType};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct Features {}

pub async fn create(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
    value: Option<String>,
    value_type: FeatureValueType,
    is_enabled: bool,
) -> anyhow::Result<Feature> {
    let mut tx = pool.begin().await?;
    let mut feature = Features::create_feature(
        &mut *tx,
        params!(environment.project_id, name, is_enabled),
        |row| Feature {
            id: row.get("id"),
            project_id: row.get("project_id"),
            is_enabled: row.get("is_enabled"),
            name: row.get("name"),
            value: None,
            value_type: flagrant_types::FeatureValueType::Text,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create a feature");
        DbError::QueryFailed
    })?;

    // store feature value within the same transaction.
    // value may vary depending on environment.

    if let Some(v) = value {
        Features::create_feature_value(
            &mut *tx,
            params![
                environment.id,
                feature.id,
                &v,
                value_type.to_string().to_lowercase()
            ],
        )
        .await?;

        feature.value = Some(v);
        feature.value_type = value_type;
    }
    tx.commit().await?;
    Ok(feature)
}

pub async fn fetch(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<Feature> {
    Ok(
        Features::fetch_feature::<_, Feature>(pool, params![environment.id, feature_id])
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch a feature");
                DbError::QueryFailed
            })?,
    )
}

pub async fn fetch_by_name(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    name: String,
) -> anyhow::Result<Feature> {
    Ok(Features::fetch_feature_by_name::<_, Feature>(
        pool,
        params![environment.id, environment.project_id, name],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch a feature");
        DbError::QueryFailed
    })?)
}

pub async fn update(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    new_name: String,
    new_value: Option<String>,
    new_value_type: FeatureValueType,
    is_enabled: bool,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    Features::update_feature(
        &mut *tx,
        params![feature.id, new_name, is_enabled],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not update a feature");
        DbError::QueryFailed
    })?;

    Features::update_feature_value(
        &mut *tx,
        params![environment.id, feature.id, new_value,  new_value_type],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not update a feature value/type");
        DbError::QueryFailed
    })?;

    tx.commit().await?;
    Ok(())
}

pub async fn list(pool: &Pool<Sqlite>, environment: &Environment) -> anyhow::Result<Vec<Feature>> {
    Ok(
        Features::fetch_features_for_environment::<_, Feature>(pool, params![environment.id, environment.project_id])
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch features for project");
                DbError::QueryFailed
            })?,
    )
}
