use anyhow::bail;
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
pub async fn upsert_default(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: String,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = Variants::upsert_default_variant(
        &mut *tx,
        params![environment.id, feature.id, &value],
        |v| v.get("variant_id"),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not upsert default variant");
        DbError::QueryFailed
    })?;

    upsert_default_weight(&mut tx, environment, feature.id).await?;
    tx.commit().await?;

    Ok(Variant::build_default(environment, variant_id, value))
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
    let variant_id = Variants::create_variant(&mut *tx, params!(feature.id, &value), |v| {
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
        tracing::error!(error = ?e, "Could not insert a variant weight");
        DbError::QueryFailed
    })?;

    upsert_default_weight(&mut tx, environment, feature.id).await?;
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
    let feature_id: u16 =
        Variants::update_variant_value(&mut *tx, params![variant.id, new_value], |v| {
            v.get("feature_id")
        })
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

    upsert_default_weight(&mut tx, environment, feature_id).await?;
    tx.commit().await?;

    Ok(())
}

/// Returns variant of given id.
///
/// Variant is returned along with its value and weight. Control variant weight is auto-calculated
/// based on sum of other feature variants within given environment.
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
    let variants = Variants::fetch_variants_for_feature::<_, Variant>(
        pool,
        params![environment.id, feature.id],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Could not fetch variants for feature");
        DbError::QueryFailed
    })?;

    // Be sure that feature has default value set within given environment.
    // No default value makes any additional variants pointless, even if they
    // already exist for other environments. Hence the Error as result.

    if !variants.iter().any(|v| is_default(environment, v)) {
        bail!(
            "No feature value set. Use \"FEATURE val {} <value>\" to set default feature value.",
            feature.name
        );
    }
    Ok(variants)
}

/// Deletes a variant.
///
/// Removes permanently variant of given `variant_id`. This function is supposed to be called
/// within the outer transaction when entire feature is being removed, hence it takes a connection
/// (instead of pool) as argument.
pub async fn delete(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let variants_count: u16 = Variants::fetch_count_of_feature_variants(
        &mut *tx,
        params![environment.id, variant.id],
        |r| r.get("count"),
    )
    .await?;

    if variants_count > 1 && is_default(environment, variant) {
        bail!("Could not remove a default variant as there still exist other variants for given feature");
    }

    Variants::delete_variant_weights(&mut *tx, params![variant.id])
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not remove variant weights");
            DbError::QueryFailed
        })?;

    let feature_id: u16 =
        Variants::delete_variant(&mut *tx, params![variant.id], |v| v.get("feature_id"))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not remove variant");
                DbError::QueryFailed
            })?;

    if !is_default(environment, variant) {
        upsert_default_weight(&mut tx, environment, feature_id).await?;
    }
    tx.commit().await?;

    Ok(())
}

pub async fn update_accumulator(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    accumulator: i16,
) -> anyhow::Result<()> {
    Variants::update_variant_accumulator(conn, params![environment.id, variant.id, accumulator])
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not update variant accumulator");
            DbError::QueryFailed
        })?;

    Ok(())
}

/// Inserts or updates feature default variant weight.
/// Weight is calculated based on sum of all the other feature variants weights within given environment.
async fn upsert_default_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<()> {
    Variants::upsert_default_variant_weight(conn, params![environment.id, feature_id])
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Could not upsert default variant weight");
            DbError::QueryFailed
        })?;

    Ok(())
}

/// Returns true if variant is default one within given environment.
/// Returns false otherwise.
fn is_default(environment: &Environment, variant: &Variant) -> bool {
    variant.environment_id.map(|id| id == environment.id).unwrap_or(false)
}
