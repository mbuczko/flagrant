use anyhow::bail;
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, Row, SqliteConnection};

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Feature, FeatureValue, IdentityVariant, Variant};

use super::identity;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/variants.sql"]
struct SQLVariants {}

/// Creates or updates the control variant of the given feature.
///
/// The control variant represents the environment-specific feature value returned when either
/// the distributor decides so based on the underlying distribution strategy, or no other
/// variant has been defined for the feature yet.
///
/// An important property of the control variant is its auto-adjustable weight, calculated
/// according to the following rules:
///
/// - when created, weight is initially set to 100%
/// - each time a new feature variant is added, modified, or removed, the control weight
///   adjusts itself so that all feature variant weights always sum to 100%.
///
/// The control variant is auto-created when a feature is created, which means
/// a newly created feature always contains at least one variant - the control one.
pub(crate) async fn create_control(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: FeatureValue,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = SQLVariants::upsert_control_variant(
        &mut *tx,
        params![environment.id, feature.id, &value],
        |v| v.get("variant_id"),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not upsert default variant", e))?;

    balance_control_weight(&mut tx, environment, feature.id, variant_id, 0).await?;
    tx.commit().await?;

    Ok(Variant::build_default(environment, variant_id, value))
}

/// Creates a feature variant with the given weight and value.
///
/// Non-control feature variants hold alternative values shared across all environments, i.e.
/// any update to a feature value is reflected immediately in all environments. Weights, on the
/// other hand, prioritize the variant during the distribution process and behave the opposite
/// way - a weight change impacts a single environment only.
pub async fn create(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    value: FeatureValue,
    weight: u8,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variant_id = SQLVariants::create_variant(&mut *tx, params![feature.id, &value], |v| {
        v.get("variant_id")
    })
    .await
    .map_err(|e| -> anyhow::Error {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return FlagrantError::BadRequest(
                "A variant with this value already exists for this feature",
            )
            .into();
        }
        FlagrantError::QueryFailed("Could not create a variant", e).into()
    })?;

    SQLVariants::upsert_variant_weight(&mut *tx, params![environment.id, variant_id, weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not insert a variant weight", e))?;

    balance_control_weight(&mut tx, environment, feature.id, variant_id, weight as i8).await?;
    tx.commit().await?;

    Ok(Variant::build(variant_id, value, weight))
}

/// Updates a variant's value (shared across environments) and weight (environment-specific).
/// Rejects updates to control variants — use `feature::update` to change the control value.
/// Returns the variant's `feature_id` for use in post-update weight balancing.
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
        SQLVariants::update_variant_value(&mut *conn, params![variant.id, new_value], |v| {
            v.get("feature_id")
        })
        .await
        .map_err(|e| -> anyhow::Error {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_unique_violation()
            {
                return FlagrantError::BadRequest(
                    "A variant with this value already exists for this feature",
                )
                .into();
            }
            FlagrantError::QueryFailed("Could not update variant value", e).into()
        })?;

    SQLVariants::upsert_variant_weight(&mut *conn, params![environment.id, variant.id, new_weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not set a variant's weight", e))?;

    Ok(feature_id)
}

/// Updates a single variant with `new_value` and `new_weight`.
///
/// Rejects modifications to the control variant, whose weight is auto-adjusted and
/// whose value can only be changed via `feature::update`.
pub async fn update_one(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    new_value: FeatureValue,
    new_weight: u8,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let feature_id = update(&mut tx, environment, variant, new_value, new_weight).await?;

    balance_control_weight(
        &mut tx,
        environment,
        feature_id,
        variant.id,
        new_weight as i8 - variant.weight as i8,
    )
    .await?;
    tx.commit().await?;

    Ok(())
}

/// Returns variant of given id.
///
/// Variant is returned along with its value and weight. Control variant weight is auto-calculated
/// based on sum of other feature variants within given environment. `segment_id` scopes which
/// weight/accumulator row is attached (`None` = organic default weights).
pub async fn get_by_id(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant_id: i32,
    segment_id: Option<i32>,
) -> anyhow::Result<Variant> {
    let variant =
        SQLVariants::fetch_variant_by_id(conn, params![environment.id, variant_id, segment_id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could fetch a variant", e))?;

    Ok(variant)
}

/// Returns the variant for a feature matching the given value, or `None` if not found.
///
/// Matches both non-control variants (environment_id IS NULL) and the environment's
/// control variant. `segment_id` scopes which weight/accumulator row is attached
/// (`None` = organic default weights).
pub async fn get_by_value(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    value: &FeatureValue,
    segment_id: Option<i32>,
) -> anyhow::Result<Option<Variant>> {
    let variant = SQLVariants::fetch_variant_by_value(
        conn,
        params![environment.id, feature_id, value, segment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could fetch a variant", e))?;

    Ok(variant)
}

/// Returns variants across all features assigned to a given identity in the given environment.
pub async fn get_by_identity<T: AsRef<str>>(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: T,
) -> anyhow::Result<Vec<IdentityVariant>> {
    let variants = SQLVariants::fetch_variants_for_identity(
        conn,
        params![environment.project_id, environment.id, identity.as_ref()],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could fetch a variant", e))?;

    Ok(variants)
}

/// Returns all variants for a feature in the given environment, including values and weights.
/// `segment_id` scopes which weight/accumulator row is attached to each variant (`None` =
/// organic default weights; `Some(id)` = that segment's override weights, sparse — variants
/// without an explicit override resolve to weight 0 / accumulator 0).
/// Errors if the feature has no default value set in this environment (which should never happen).
pub async fn get_for_feature(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    segment_id: Option<i32>,
) -> anyhow::Result<Vec<Variant>> {
    let variants = SQLVariants::fetch_variants_for_feature(
        conn,
        params![environment.id, feature_id, segment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch variants for feature", e))?;

    // Ensure the feature has a default value set in the given environment.
    // Without a default value, any additional variants are pointless, even if they
    // already exist in other environments - hence the error result.
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
    let (feature_id, variants_count): (i32, i8) =
        SQLVariants::fetch_count_of_feature_variants(&mut *tx, params![environment.id, variant.id])
            .await?;

    if variants_count > 1 && is_default(environment, variant) {
        bail!(FlagrantError::BadRequest(
            "Could not remove control variant as there are still other variants existing for this feature"
        ));
    }

    // All identities must be detached from the variant first to ensure
    // there are no dangling references to the given variant_id.
    identity::detach_identities(&mut tx, variant.id).await?;

    // Remove all weight entries attached to this variant.
    SQLVariants::delete_variant_weights(&mut *tx, params![variant.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not remove variant weights", e))?;

    SQLVariants::delete_variant(&mut *tx, params![variant.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not remove variant", e))?;

    if !is_default(environment, variant) {
        balance_control_weight(&mut tx, environment, feature_id, variant.id, -100).await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Sets the weight of an existing non-control variant for `environment`.
///
/// Used when inheriting variant weights from a base environment into a newly created one.
pub(crate) async fn set_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant_id: i32,
    weight: u8,
) -> anyhow::Result<()> {
    SQLVariants::upsert_variant_weight(conn, params![environment.id, variant_id, weight])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not set variant weight", e))?;
    Ok(())
}

pub(crate) async fn update_accumulator(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant: &Variant,
    segment_id: Option<i32>,
    accumulator: i32,
) -> anyhow::Result<()> {
    SQLVariants::update_variant_accumulator(
        conn,
        params![environment.id, variant.id, segment_id, accumulator],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not update variant accumulator", e))?;

    Ok(())
}

/// Sets a segment-scoped weight for an existing non-control variant, similarly to how
/// organic weights are set via [`set_weight`].
pub(crate) async fn set_segment_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    segment_id: i32,
    variant_id: i32,
    weight: u8,
) -> anyhow::Result<()> {
    SQLVariants::upsert_segment_variant_weight(
        conn,
        params![environment.id, variant_id, segment_id, weight],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not set segment variant weight", e))?;
    Ok(())
}

/// Recalculates and persists the control variant's remainder weight within a segment,
/// mirroring [`recalculate_control_weight`] for the organic case. Should be called after
/// all of a segment's explicit override weights have been written via [`set_segment_weight`].
///
/// Unlike the organic path, this does not migrate already-assigned identities — segment-scoped
/// identity assignment tracking doesn't exist yet.
pub(crate) async fn balance_segment_control_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    segment_id: i32,
    feature_id: i32,
) -> anyhow::Result<(i32, u8)> {
    Ok(
        SQLVariants::upsert_segment_control_variant_weight::<_, (i32, u8)>(
            conn,
            params![environment.id, feature_id, segment_id],
        )
        .await
        .map_err(|e| {
            FlagrantError::QueryFailed("Could not recalculate segment control variant weight", e)
        })?,
    )
}

/// Returns the stored explicit weight overrides for a given segment+feature+environment
/// (excludes the control variant's auto-balanced remainder row).
pub async fn get_segment_weights(
    conn: &mut SqliteConnection,
    segment_id: i32,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Vec<(i32, u8)>> {
    SQLVariants::fetch_segment_variant_weights(
        conn,
        params![segment_id, feature_id, environment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch segment variant weights", e).into())
}

/// Returns `(segment_name, variant_id, weight)` for all segments overriding a feature+environment
/// (excludes the control variant's auto-balanced remainder row).
pub async fn get_segment_overrides_with_weights(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Vec<(String, i32, u8)>> {
    SQLVariants::fetch_segment_overrides_with_weights(conn, params![feature_id, environment_id])
        .await
        .map_err(|e| {
            FlagrantError::QueryFailed("Could not fetch segment overrides for feature", e).into()
        })
}

/// Removes all segment-scoped weight overrides (including the control variant's remainder
/// row) for a segment+feature+environment.
pub(crate) async fn delete_segment_weights_for_feature(
    conn: &mut SqliteConnection,
    segment_id: i32,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<()> {
    SQLVariants::delete_segment_variant_weights_for_feature(
        conn,
        params![segment_id, feature_id, environment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not clear segment overrides", e))?;
    Ok(())
}

/// Recalculates and persists the control variant weight for `feature_id` in `environment`.
///
/// The new weight is computed as `100 - sum(non_control_weights)`. Should be called after
/// all non-control variant weights for the environment have been inserted.
///
/// Returns `(control_variant_id, new_weight)`.
pub(crate) async fn recalculate_control_weight(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<(i32, u8)> {
    Ok(SQLVariants::upsert_control_variant_weight::<_, (i32, u8)>(
        conn,
        params![environment.id, feature_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not recalculate control variant weight", e))?)
}

/// Upserts the control variant to balance weight between control variant and given variant_id.
///
/// If `weight` is associated with a `variant_id` other than the control variant, a positive diff
/// decreases the control variant weight - as if the weight were moved from the control variant to
/// `variant_id`. Conversely, a negative weight bumps up the control variant - as if `variant_id`
/// were giving its weight back to the control variant.
///
/// This function also handles identities already attached to affected variants: if the count of
/// attached identities, expressed as a percentage, exceeds the new weight, the excess identities
/// are marked as detached starting from the earliest ones, so the distributor can reassign them.
///
/// Returns the new control variant weight.
async fn balance_control_weight(
    tx: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    variant_id: i32,
    weight_diff: i8,
) -> anyhow::Result<(i32, u8)> {
    let (control_variant_id, control_weight) =
        recalculate_control_weight(tx, environment, feature_id).await?;

    let (from_id, to_id) = if weight_diff > 0 {
        (control_variant_id, variant_id)
    } else {
        (variant_id, control_variant_id)
    };
    if variant_id != control_variant_id {
        identity::migrate_identities(
            &mut *tx,
            environment,
            from_id,
            to_id,
            weight_diff.unsigned_abs(),
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
