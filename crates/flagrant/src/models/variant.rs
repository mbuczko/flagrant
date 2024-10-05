use anyhow::bail;
use hugsqlx::{params, HugSqlx};
use sqlx::{Connection, Row, SqliteConnection};

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Feature, FeatureValue, IdentityVariant, Variant};

use super::identity;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/variants.sql"]
struct Variants {}

/// Creates or updates control variant of given feature.
///
/// Control variant represents environment-specific feature value being returned when either
/// distributor decided so based on underlaying distribution strategy, or simply no other
/// variant has been defined yet. It's important to understand that control variant weight is
/// an auto-adjustable value, being calculated according to following rules:
///
/// - when created, weight is initially set up to 100%
/// - each time new feature variant is being added, modified or removed control weight adjusts
///   itself so, that all feature variants weights at every single moment sum to 100%.
///
/// Control variant, similar to standard variants is optional. No such a variant simply means
/// that feature has no default value defined yet. This also comes with important consequence -
/// it is impossible to create additional variants having no control variant created in a first
/// place.
pub async fn create_control(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: FeatureValue,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = Variants::upsert_default_variant(
        &mut *tx,
        params![environment.id, feature.id, &value],
        |v| v.get("variant_id"),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not upsert default variant", e))?;

    update_control_weight(&mut tx, environment, feature.id).await?;
    tx.commit().await?;

    Ok(Variant::build_default(environment, variant_id, value))
}

/// Creates a feature variant with given weight and value.
///
/// Non-control feature variants hold an alternative values shared by defined environments, ie.
/// any update on feature value impacts immediately all the environments. Weights however behave
/// exactly the opposite way - the change impacts given environment only and similarly to control,
/// variant is used to determine how to prioritize variant during distribution process.
pub async fn create(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: FeatureValue,
    weight: u8,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = Variants::create_variant(&mut *tx, params![feature.id, &value], |v| {
        v.get("variant_id")
    })
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not create a variant", e))?;

    Variants::upsert_variant_weight(&mut *tx, params![environment.id, variant_id, weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not insert a variant weight", e))?;

    // each newly added variant decreases control variant weight
    update_control_weight(&mut tx, environment, feature.id).await?;
    tx.commit().await?;

    Ok(Variant::build(variant_id, value, weight))
}

/// Updates feature variant with `new_value` and `new_weight`.
///
/// This function fails-fast when used to modify control variant which, because of auto-adjustable
/// weight, should be altered with `feature::update` function instead.
pub async fn update(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    new_value: FeatureValue,
    new_weight: u8,
) -> anyhow::Result<()> {
    if variant.is_control() {
        bail!("Control variant cannot be updated with new weight as it auto-adjusts itself.");
    }

    let mut tx = conn.begin().await?;
    let feature_id: u16 =
        Variants::update_variant_value(&mut *tx, params![variant.id, new_value], |v| {
            v.get("feature_id")
        })
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update variant value", e))?;

    Variants::upsert_variant_weight(&mut *tx, params![environment.id, variant.id, new_weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not set a variant's weight", e))?;

    // identity::detach_identities(&mut tx, environment, variant, new_weight).await?;

    update_control_weight(&mut tx, environment, feature_id).await?;

    tx.commit().await?;

    Ok(())
}

/// Returns variant of given id.
///
/// Variant is returned along with its value and weight. Control variant weight is auto-calculated
/// based on sum of other feature variants within given environment.
pub async fn get_by_id(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant_id: u16,
) -> anyhow::Result<Variant> {
    let variant = Variants::fetch_variant(conn, params![environment.id, variant_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could fetch a variant", e))?;

    Ok(variant)
}

/// Returns variant assigned to given identity
pub async fn get_by_identity<T: AsRef<str>>(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: T,
) -> anyhow::Result<Vec<IdentityVariant>> {
    let variants =
        Variants::fetch_variants_for_identity(conn, params![environment.id, identity.as_ref()])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could fetch a variant", e))?;

    Ok(variants)
}

/// Returns all variants of given feature.
///
/// Variants are returned along with their values and weights. Note that control variant's weight
/// is calculated dynamically based on the sum of the other variants, it's not persisted directy
/// in database.
pub async fn get_all(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<Vec<Variant>> {
    let variants = Variants::fetch_variants_for_feature(conn, params![environment.id, feature_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch variants for feature", e))?;

    // Be sure that feature has default value set within given environment.
    // No default value makes any additional variants pointless, even if they
    // already exist for other environments - hence the Error as result.

    if !variants.iter().any(|v| is_default(environment, v)) {
        bail!(FlagrantError::BadRequest(
            "No feature value set. Use \"FEATURE val ...\" to set default feature value."
        ));
    }
    Ok(variants)
}

/// Permanently deletes a variant of given id.
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
        bail!(FlagrantError::BadRequest("Could not remove default variant as there are still other variants defined for this feature"));
    }

    Variants::delete_variant_weights(&mut *tx, params![variant.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not remove variant weights", e))?;

    let feature_id: u16 =
        Variants::delete_variant(&mut *tx, params![variant.id], |v| v.get("feature_id"))
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not remove variant", e))?;

    if !is_default(environment, variant) {
        update_control_weight(&mut tx, environment, feature_id).await?;
    }
    tx.commit().await?;

    Ok(())
}

pub async fn update_accumulator(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    accumulator: i32,
) -> anyhow::Result<()> {
    Variants::update_variant_accumulator(conn, params![environment.id, variant.id, accumulator])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update variant accumulator", e))?;

    Ok(())
}

/// Inserts or updates feature control variant weight.
/// Weight is calculated based on sum of all the other feature variants weights within given environment.
/// Returns new control variant weight.
async fn update_control_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<u8> {
    let (variant_id, weight, weight_diff) = Variants::upsert_default_variant_weight::<_, (u16, u8, Option<i8>)>(&mut *conn, params![environment.id, feature_id])
        .await
        .map_err(|e| {
            FlagrantError::QueryFailed("Could not upsert default variant weight", e)
        })?;

    if let Some(diff) = weight_diff && diff.is_negative() {
        identity::detach_identities(conn, environment, variant_id, weight).await?;
    }
    Ok(weight)
}

/// Returns true if variant is default one within given environment.
fn is_default(environment: &Environment, variant: &Variant) -> bool {
    variant
        .environment_id
        .map(|id| id == environment.id)
        .unwrap_or(false)
}
