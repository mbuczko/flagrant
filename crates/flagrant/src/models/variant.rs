use std::cmp::max;

use hugsqlx::{params, HugSqlx};
use sqlx::{Connection, Row, SqliteConnection};
use sqlx::{Pool, Sqlite};

use crate::errors::DbError;
use flagrant_types::{Environment, Feature, Variant};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/variants.sql"]
struct Variants {}

/// Creates or updates default (control) variant of given feature.
///
/// Default variant represents environment-specific feature control value ie. a value which may
/// differ across environments. There is also no weight information assigned as default variant's
/// weight is calculated dynamically based on sum of other variants weights.
///
/// Weight is used to determine how to prioritize variant during request balancing process.
pub async fn upsert_default(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: String,
) -> anyhow::Result<Variant> {
    let variant_id =
        Variants::upsert_default_variant(conn, params![environment.id, feature.id, &value], |v| {
            v.get("variant_id")
        })
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not upsert default variant");
            DbError::QueryFailed
        })?;

    Ok(Variant::build_default(variant_id, value))
}

/// Creates standard variant with weight and value common for all environments.
///
/// In oppose to default (control) one, standard variant holds an alternative value which is
/// common across all environments, ie. once changed, it's changed immediately for all environments.
///
/// Weight on the other hand is environment-specific, so the change impacts given environment only
/// and is used to determine how to prioritize variant during request balancing process.
pub async fn create(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    value: String,
    weight: i16,
) -> anyhow::Result<Variant> {
    let mut tx = pool.begin().await?;
    let variant_id =
        Variants::create_standard_variant(&mut *tx, params!(feature.id, &value), |v| {
            v.get("variant_id")
        })
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not create a variant");
            DbError::QueryFailed
        })?;

    let weight = Variants::upsert_variant_weight(
        &mut *tx,
        params![environment.id, variant_id, weight],
        |v| v.get("weight"),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not set a variant weight");
        DbError::QueryFailed
    })?;

    tx.commit().await?;
    Ok(Variant::build(variant_id, value, weight))
}

/// Updates standard variant.
///
/// Standard variant represents alternative feature value common across environments and is chosen
/// based on environment-specific weight during request balancing process.
pub async fn update(
    pool: &Pool<Sqlite>,
    environment: &Environment,
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

    Variants::upsert_variant_weight::<_, _, i16>(
        &mut *tx,
        params![environment.id, variant.id, new_weight],
        |v| v.get("weight"),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not set a variant's weight");
        DbError::QueryFailed
    })?;

    tx.commit().await?;
    Ok(())
}

pub async fn fetch(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    variant_id: u16,
) -> anyhow::Result<Variant> {
    let variant = Variants::fetch_variant::<_, Variant>(pool, params!(environment.id, variant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could fetch a variant");
            DbError::QueryFailed
        })?;

    Ok(variant)
}

pub async fn list(
    pool: &Pool<Sqlite>,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<Vec<Variant>> {
    let mut variants = Variants::fetch_variants_for_feature::<_, Variant>(
        pool,
        params!(environment.id, feature.id),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch variants for feature");
        DbError::QueryFailed
    })?;

    let sum_weights = variants.iter().fold(0, |acc, v| acc + v.weight);
    if let Some(v) = variants.first_mut() {
        v.weight = max(0, 100 - sum_weights)
    }
    Ok(variants)
}

/// Deletes a variant.
///
/// This function exceptionally (compared to other functions in this namespace)
/// takes as argument `SqliteConnection` instead of `Pool`. This is because it's
/// also being used in feature removal which calls this code in a sub-transaction.
/// This requires both - outer transaction and subtransaction to operate on same
/// connection.
pub async fn delete(conn: &mut SqliteConnection, variant_id: u16) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

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

// Transforms database result serialized as `SqliteRow` into a `Variant` model.
// pub(crate) fn row_to_variant(row: SqliteRow) -> Variant {
//     Variant {
//         id: row.get("variant_id"),
//         value: row.get("value"),
//         weight: row.get("weight"),
//         accumulator: row.get("accumulator"),
//         is_control: row.get("is_control")
//     }
// }
