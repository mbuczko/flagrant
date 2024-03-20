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
    weight: u16,
) -> anyhow::Result<Variant> {
    let variant_id =
        Variants::create_variant(pool, params!(feature.id, &value), |v| v.get("variant_id"))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not create a variant");
                DbError::QueryFailed
            })?;

    let weight = Variants::create_variant_weight(pool, params![env.id, variant_id, weight], |v| {
        v.get("weight")
    })
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not set a variant weight");
        DbError::QueryFailed
    })?;

    Ok(Variant {
        id: variant_id,
        value,
        weight,
        acc: 100,
    })
}

pub async fn fetch(
    pool: &Pool<Sqlite>,
    env: &Environment,
    variant_id: u16,
) -> anyhow::Result<Variant> {
    Ok(
        Variants::fetch_variant::<_, Variant>(pool, params!(env.id, variant_id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could fetch a variant");
                DbError::QueryFailed
            })?,
    )
}

pub async fn list(
    pool: &Pool<Sqlite>,
    env: &Environment,
    feature: &Feature,
) -> anyhow::Result<Vec<Variant>> {
    Ok(
        Variants::fetch_variants_for_feature::<_, Variant>(pool, params!(env.id, feature.id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch variants for feature");
                DbError::QueryFailed
            })?,
    )
}

pub async fn delete(pool: &Pool<Sqlite>, variant_id: u16) -> anyhow::Result<()> {
    Variants::delete_variant(pool, params!(variant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not remove variant");
            DbError::QueryFailed
        })?;

    Ok(())
}
