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
/// Default variant represents environment-specific feature control value ie. a value returned
/// when no other variants have been defined or, having multiple other variants already added,
/// when distributor decides to prioritize it over other variants based on weight and underlaying
/// distributing strategy.
///
/// Default variant, similar to standard variants is optional. No such a variant means that feature
/// has no value defined. Also, having no default variant it's impossible to create other variants.
///
/// Note that control variant is special when it comes to its weight - it has no weight information
/// persisted in database. Instead, weight is calculated dynamically on feature featch operations.
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
/// In oppose to default (control) one, standard variants hold an alternative value common across all
/// environments, ie. once changed, value is propagated immediately to all environments. Weight on the
/// other hand is environment-specific, so the change impacts given environment only and, similarly to
/// default variant, is used to determine how to prioritize variant during distribution process.
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

/// Updates standard variant with `new_value` and `new_weight`.
///
/// Standard variant represents alternative feature value which is common across environments
/// and, based on weight and distribution strategy, may be prioritized over other variants
/// during distribution process.
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

/// Returns variant of given id.
///
/// Variant is returned along with its value and weight. Control variant is a minor exception as its
/// weight is not persisted - it's calculated dynamically during feature fetch operations.
/// When fetched  directly, control variant's weight becomes 0.
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

/// Returns all variants of given feature.
///
/// Variants are returned along with their values and weights. Note that control variant's weight
/// is calculated dynamically based on the sum of the other variants, it's not persisted directy
/// in database.
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
/// Removes permanently variant of given `variant_id`. This function is supposed to be called
/// within the outer transaction when entire feature is being removed, hence it takes a connection
/// (instead of pool) as argument.
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
