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
///   itself so, that all feature variants weights at every single moment sum up to 100%.
///
/// Control variant is auto-created at the moment when feature is being created which means
/// that newly created feature already contains at least a single variant - a control one.
pub(crate) async fn create_control(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: FeatureValue,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = Variants::upsert_control_variant(
        &mut *tx,
        params![environment.id, feature.id, &value],
        |v| v.get("variant_id"),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not upsert default variant", e))?;

    upsert_control_weight(&mut tx, environment, feature.id, variant_id, 0).await?;
    tx.commit().await?;

    Ok(Variant::build_default(environment, variant_id, value))
}

/// Creates a feature variant with given weight and value.
///
/// Non-control feature variants hold an alternative values shared by defined environments, ie.
/// any update on feature value is reflected immediately in all environments. Weights on the
/// other hand prioritize the variant during distribution process and behave exactly the opposite
/// way - the change impacts single environment only.
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

    upsert_control_weight(&mut tx, environment, feature.id, variant_id, weight as i8).await?;
    tx.commit().await?;

    Ok(Variant::build(variant_id, value, weight))
}

async fn update(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    new_value: FeatureValue,
    new_weight: u8,
) -> anyhow::Result<i32> {
    if variant.is_control() {
        bail!("Control variant is immutable. Use feature::update to adjust its value.");
    }
    let feature_id: i32 =
        Variants::update_variant_value(&mut *conn, params![variant.id, new_value], |v| {
            v.get("feature_id")
        })
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not update variant value", e))?;

    Variants::upsert_variant_weight(&mut *conn, params![environment.id, variant.id, new_weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not set a variant's weight", e))?;

    Ok(feature_id)
}

/// Updates a single variant with `new_value` and `new_weight`.
///
/// This function fails-fast when used to modify control variant which, due to auto-adjustable
/// nature, is immutable and should be altered by `feature::update` function instead.
pub async fn update_one(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    new_value: FeatureValue,
    new_weight: u8,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let feature_id = update(&mut tx, environment, variant, new_value, new_weight).await?;

    upsert_control_weight(
        &mut tx,
        environment,
        feature_id,
        variant.id,
        new_weight as i8,
    )
    .await?;
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
    variant_id: i32,
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

/// Returns all variants of given feature. Variants are returned along with their values and weights.
/// Returns Error when no feature value has been set.
pub async fn get_all(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
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

/// Permanently deletes a variant of given id and triggers control variant weight update.
///
/// When deleting variants, control variant should be deleted as a last one - when no other
/// variants already exist. Otherwise an Error gets returned.
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
        bail!(FlagrantError::BadRequest("Could not remove control variant as there are still other variants existing for this feature"));
    }

    Variants::delete_variant_weights(&mut *tx, params![variant.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not remove variant weights", e))?;

    let feature_id: i32 =
        Variants::delete_variant(&mut *tx, params![variant.id], |v| v.get("feature_id"))
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not remove variant", e))?;

    if !is_default(environment, variant) {
        upsert_control_weight(
            &mut tx,
            environment,
            feature_id,
            variant.id,
            -(variant.weight as i8),
        )
        .await?;
    }
    tx.commit().await?;

    Ok(())
}

pub(crate) async fn update_accumulator(
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

/// Inserts or updates control variant to accomadate given weight.
///
/// How accomodation works?
/// If `weight` is associated with `variant_id` other than control variant, positive value decreases
/// control variant. Imagine this as if the weight would need to be moved from control variant to
/// `variant_id`. Conversely, negative weight bumps up control variant - just as if `variant_id` would
/// give its weight back to control variant.
///
/// This function also takes care of number of already attached identities which, taken as percentage,
/// may happen to exceed the new weight. In this case, exceeding identities are marked as "detatched"
/// starting from the earliest attached ones and thus may be re-assigned by distributor to other variants.
///
/// Returns new control variant weight.
async fn upsert_control_weight(
    tx: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    variant_id: i32,
    variant_weight: i8,
) -> anyhow::Result<(i32, u8)> {
    let (control_variant_id, control_weight) = Variants::upsert_control_variant_weight::<
        _,
        (i32, u8),
    >(&mut *tx, params![environment.id, feature_id])
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not upsert default variant weight", e))?;

    if variant_id != control_variant_id {
        identity::migrate_attached(
            &mut *tx,
            environment,
            control_variant_id,
            variant_id,
            (control_weight as i8) - variant_weight,
        )
        .await?;
    }

    Ok((control_variant_id, control_weight))
}

/// Returns true if variant is default one within given environment.
fn is_default(environment: &Environment, variant: &Variant) -> bool {
    variant
        .environment_id
        .map(|id| id == environment.id)
        .unwrap_or(false)
}
