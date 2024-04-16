use hugsqlx::{params, HugSqlx};
use sqlx::Row;
use sqlx::{Pool, Sqlite};

use crate::errors::DbError;
use flagrant_types::{Environment, Feature, Variant};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/variants.sql"]
struct Variants {}

pub async fn create(
    pool: &Pool<Sqlite>,
    env: &Environment,
    feature: &Feature,
    value: String,
    weight: i16,
) -> anyhow::Result<Variant> {
    let mut tx = pool.begin().await?;
    let variant_id = Variants::create_variant(&mut *tx, params!(feature.id, &value), |v| {
        v.get("variant_id")
    })
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not create a variant");
        DbError::QueryFailed
    })?;

    let weight =
        Variants::upsert_variant_weight(&mut *tx, params![env.id, variant_id, weight], |v| {
            v.get("weight")
        })
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not set a variant weight");
            DbError::QueryFailed
        })?;

    tx.commit().await?;
    Ok(Variant {
        id: variant_id,
        value,
        weight,
        acc: 100,
    })
}

pub async fn update(
    pool: &Pool<Sqlite>,
    env: &Environment,
    variant: &Variant,
    new_value: String,
    new_weight: i16,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    Variants::update_variant_value(&mut *tx, params!(variant.id, new_value))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not update variant value");
            DbError::QueryFailed
        })?;

    Variants::upsert_variant_weight::<_, _, i16>(&mut *tx, params![env.id, variant.id, new_weight], |v| {
            v.get("weight")
        })
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not set a variant weight");
            DbError::QueryFailed
        })?;

    tx.commit().await?;
    Ok(())
}

pub async fn fetch(
    pool: &Pool<Sqlite>,
    env: &Environment,
    variant_id: u16,
) -> anyhow::Result<Variant> {
    let variant = Variants::fetch_variant::<_, Variant>(pool, params!(env.id, variant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could fetch a variant");
            DbError::QueryFailed
        })?;

    Ok(variant)
}

pub async fn list(
    pool: &Pool<Sqlite>,
    env: &Environment,
    feature: &Feature,
) -> anyhow::Result<Vec<Variant>> {
    let variants =
        Variants::fetch_variants_for_feature::<_, Variant>(pool, params!(env.id, feature.id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch variants for feature");
                DbError::QueryFailed
            })?;

    Ok(variants)
}

pub async fn delete(pool: &Pool<Sqlite>, variant_id: u16) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    Variants::delete_variant_weights(&mut *tx, params!(variant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not remove variant weights");
            DbError::QueryFailed
        })?;

    Variants::delete_variant(&mut *tx, params!(variant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not remove variant");
            DbError::QueryFailed
        })?;

    tx.commit().await?;
    Ok(())
}
