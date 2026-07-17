use chrono::{NaiveDateTime, Utc};
use flagrant_types::payload::{IdentityPatch, IdentityTraitPayload, TraitPatchOp};
use flagrant_types::{
    Environment, FeatureOverride, FeatureValue, Identity, IdentityTrait, IdentityVariant,
    IdentityWithTraits, TraitValue,
};

use super::feature;
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use smallvec::SmallVec;
use sqlx::{Connection, SqliteConnection};

use crate::{distributor, errors::FlagrantError, evaluator};

use super::surround_string;
use super::traits::upsert;
use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/identities.sql"]
pub struct SQLIdentities {}

#[derive(sqlx::FromRow)]
struct IdentityWithTraitRow {
    identity_id: i32,
    identity: String,
    trait_id: Option<i32>,
    trait_name: Option<String>,
    trait_value: Option<String>,
}

/// A single trait filter condition used by [`list`]: matches identities carrying a trait
/// named `name`. If constructed via [`TraitCondition::value`], only a value that coerces
/// to the given raw string matches - trying every plausible type (bool/int/float) plus a
/// plain-string fallback, so the caller doesn't need to know how the value was originally
/// typed (e.g. `experimental=true` matches both `bool::true` and `str::true`). If
/// constructed via [`TraitCondition::any_value`], any value matches - and for exclusions,
/// this means "does not have this trait at all".
pub struct TraitCondition<'a> {
    name: &'a str,
    /// Candidate type-encoded values (as produced by `TraitValue::to_string()`), any one
    /// of which counts as a match. `None` means any value (or no value) matches.
    values: Option<Vec<String>>,
}

impl<'a> TraitCondition<'a> {
    pub fn any_value(name: &'a str) -> Self {
        Self { name, values: None }
    }

    pub fn value(name: &'a str, raw: &str) -> Self {
        let mut candidates = Vec::with_capacity(4);
        if let Ok(b) = raw.parse::<bool>() {
            candidates.push(TraitValue::Bool(b).to_string());
        }
        if let Ok(i) = raw.parse::<i32>() {
            candidates.push(TraitValue::Int(i).to_string());
        }
        if let Ok(f) = raw.parse::<f32>() {
            candidates.push(TraitValue::Float(f).to_string());
        }
        candidates.push(TraitValue::Str(raw.to_owned()).to_string());

        Self {
            name,
            values: Some(candidates),
        }
    }
}

/// Encodes trait conditions as a JSON array of `[name, [value, ...] | null]` entries,
/// suitable for SQLite's `json_each()`. Returns `None` when `conditions` is `None`.
fn conditions_into_json_string(
    conditions: Option<SmallVec<[TraitCondition<'_>; 3]>>,
) -> Option<String> {
    conditions.map(|conds| {
        let items: Vec<String> = conds
            .iter()
            .map(|c| {
                let values = match &c.values {
                    Some(vals) => {
                        let quoted: Vec<String> =
                            vals.iter().map(|v| surround_string(v, '"', '"')).collect();
                        surround_string(&quoted.join(","), '[', ']')
                    }
                    None => "null".to_owned(),
                };
                format!("[{},{values}]", surround_string(c.name, '"', '"'))
            })
            .collect();

        surround_string(&items.join(","), '[', ']')
    })
}

// Helper to load traits for an identity_id
async fn load_traits(
    conn: &mut SqliteConnection,
    identity_id: i32,
) -> anyhow::Result<Vec<IdentityTrait>> {
    SQLIdentities::fetch_identity_traits::<_, IdentityTrait>(conn, params![identity_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity traits", e).into())
}

/// Lists up to 10 identities with their traits, optionally filtered by pattern and/or by
/// trait conditions. `traits_included` restricts results to identities matching at least
/// one of the given conditions; `traits_excluded` drops identities matching any of them.
pub async fn list(
    conn: &mut SqliteConnection,
    environment: &Environment,
    pattern: Option<String>,
    traits_included: Option<SmallVec<[TraitCondition<'_>; 3]>>,
    traits_excluded: Option<SmallVec<[TraitCondition<'_>; 3]>>,
) -> anyhow::Result<Vec<IdentityWithTraits>> {
    let like = pattern.unwrap_or_else(|| "%".to_string());
    let has_included = traits_included.as_ref().is_some_and(|t| !t.is_empty());
    let has_excluded = traits_excluded.as_ref().is_some_and(|t| !t.is_empty());

    let rows = SQLIdentities::fetch_identities_with_traits::<_, IdentityWithTraitRow>(
        conn,
        |cond_id| match cond_id {
            FetchIdentitiesWithTraits::TraitsIncluded => has_included,
            FetchIdentitiesWithTraits::TraitsExcluded => has_excluded,
        },
        params![
            environment.project_id,
            environment.id,
            like,
            conditions_into_json_string(traits_included),
            conditions_into_json_string(traits_excluded)
        ],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not list identities", e))?;

    let mut result: Vec<IdentityWithTraits> = Vec::new();
    for row in rows {
        match result.last_mut() {
            Some(last) if last.id == row.identity_id => {
                if let (Some(trait_id), Some(name)) = (row.trait_id, row.trait_name) {
                    last.traits.push(IdentityTrait {
                        trait_id,
                        name,
                        value: row.trait_value.and_then(|v| v.parse().ok()),
                    });
                }
            }
            _ => {
                let traits = match (row.trait_id, row.trait_name) {
                    (Some(trait_id), Some(name)) => {
                        vec![IdentityTrait {
                            trait_id,
                            name,
                            value: row.trait_value.and_then(|v| v.parse().ok()),
                        }]
                    }
                    _ => vec![],
                };
                result.push(IdentityWithTraits {
                    id: row.identity_id,
                    value: row.identity,
                    traits,
                });
            }
        }
    }
    Ok(result)
}

/// Returns an existing identity or creates a new one if it doesn't exist yet.
pub async fn get_or_create_by_value(
    conn: &mut SqliteConnection,
    environment: &Environment,
    value: String,
) -> anyhow::Result<Identity> {
    let (id, value, environment_id) = SQLIdentities::upsert_identity::<_, (i32, String, i32)>(
        conn,
        params![environment.id, value],
    )
    .await?;
    Ok(Identity {
        id,
        value,
        environment_id,
    })
}

/// Fetches a single identity with no traits by environment and identity value
pub async fn get_by_value(
    conn: &mut SqliteConnection,
    environment: &Environment,
    value: String,
) -> anyhow::Result<Identity> {
    let (id, val, environment_id) =
        SQLIdentities::fetch_identity_by_value::<_, (i32, String, i32)>(
            &mut *conn,
            params![environment.id, value],
        )
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity", e))?;

    Ok(Identity {
        id,
        value: val,
        environment_id,
    })
}

/// Fetches a single identity with its traits by identity value
pub async fn get_by_value_with_traits(
    conn: &mut SqliteConnection,
    environment: &Environment,
    value: String,
) -> anyhow::Result<IdentityWithTraits> {
    let (id, value, _) = SQLIdentities::fetch_identity_by_value::<_, (i32, String, i32)>(
        &mut *conn,
        params![environment.id, value],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity", e))?;

    Ok(IdentityWithTraits {
        id,
        value,
        traits: load_traits(conn, id).await?,
    })
}

/// Creates a new identity with optional traits. Auto-creates traits that don't exist yet.
pub async fn create(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: String,
    trait_payloads: Vec<IdentityTraitPayload>,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;
    let identity = get_or_create_by_value(&mut tx, environment, identity).await?;

    for t in trait_payloads {
        attach_trait(
            &mut tx,
            environment.project_id,
            identity.id,
            t.name,
            t.value,
        )
        .await?;
    }
    tx.commit().await?;

    let traits = load_traits(conn, identity.id).await?;
    Ok(IdentityWithTraits {
        id: identity.id,
        value: identity.value,
        traits,
    })
}

/// Replaces all traits for an identity. Auto-creates traits that don't exist yet.
pub async fn update_traits(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: Identity,
    trait_payloads: Vec<IdentityTraitPayload>,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;

    SQLIdentities::delete_identity_traits(&mut *tx, params![identity.id]).await?;
    for t in trait_payloads {
        attach_trait(
            &mut tx,
            environment.project_id,
            identity.id,
            t.name,
            t.value,
        )
        .await?;
    }

    tx.commit().await?;
    get_by_value_with_traits(conn, environment, identity.value).await
}

/// Applies a patch to an identity - applies granular trait operations
/// and pins the identity to specific variants (overrides) per feature.
pub async fn patch(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: Identity,
    patch: IdentityPatch,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;

    for op in patch.traits {
        match op {
            TraitPatchOp::Add { name, value } | TraitPatchOp::SetValue { name, value } => {
                attach_trait(&mut tx, environment.project_id, identity.id, name, value).await?;
            }
            TraitPatchOp::Delete { name } => {
                SQLIdentities::delete_identity_trait_by_name(
                    &mut *tx,
                    params![identity.id, environment.project_id, name],
                )
                .await
                .map_err(|e| {
                    FlagrantError::QueryFailed("Could not delete trait from identity", e)
                })?;
            }
        }
    }

    for ovr in patch.overrides {
        let feat = feature::get_by_name(&mut tx, environment, ovr.feature_name).await?;
        let fv: FeatureValue = ovr
            .variant_value
            .parse()
            .unwrap_or_else(|_| FeatureValue::build(&ovr.variant_value));

        let variant = variant::get_by_value(&mut tx, environment, feat.id, &fv, None)
            .await?
            .ok_or(FlagrantError::BadRequest(
                "No variant with given value found for this feature",
            ))?;

        SQLIdentities::upsert_identity_variant(
            &mut *tx,
            params![
                identity.id,
                environment.id,
                feat.id,
                variant.id,
                Option::<i32>::None,
                Some(Utc::now())
            ],
        )
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not set variant override", e))?;
    }

    for feature_name in patch.unpins {
        let feat = feature::get_by_name(&mut tx, environment, feature_name).await?;
        SQLIdentities::delete_identity_variant_for_feature(
            &mut *tx,
            params![identity.id, feat.id, environment.id],
        )
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not unpin identity variant", e))?;
    }

    tx.commit().await?;
    get_by_value_with_traits(conn, environment, identity.value).await
}

/// Deletes an identity and all associated data.
pub async fn delete(conn: &mut SqliteConnection, identity: Identity) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    SQLIdentities::delete_identity_traits(&mut *tx, params![identity.id]).await?;
    SQLIdentities::delete_identity_variants(&mut *tx, params![identity.id]).await?;
    SQLIdentities::delete_identity(&mut *tx, params![identity.id]).await?;

    tx.commit().await?;
    Ok(())
}

/// Deletes every identity (and its traits/variant assignments) in `environment` whose value
/// matches `pattern`. Pattern translates to SQL LIKE pattern - `*` becomes `%`.
pub async fn clear_matching(
    conn: &mut SqliteConnection,
    environment: &Environment,
    pattern: &str,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    SQLIdentities::delete_identity_traits_for_environment_pattern(
        &mut *tx,
        params![environment.id, pattern],
    )
    .await?;
    SQLIdentities::delete_identity_variants_for_environment_pattern(
        &mut *tx,
        params![environment.id, pattern],
    )
    .await?;
    SQLIdentities::delete_identities_for_environment_pattern(
        &mut *tx,
        params![environment.id, pattern],
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Clears variant assignments for `feature_id`, for every identity in `environment` whose
/// value matches `pattern` (SQL LIKE pattern - `*` becomes `%`), freeing them to be
/// redistributed on the next evaluation. Unlike [`clear_matching`], the identities themselves
/// (and their traits) are left untouched - only their assignment to this feature is removed.
pub async fn clear_distribution_for_feature(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    pattern: &str,
) -> anyhow::Result<()> {
    SQLIdentities::delete_identity_variants_for_feature_pattern(
        conn,
        params![feature_id, environment.id, pattern],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not clear variant assignments", e))?;

    Ok(())
}

/// Flags every already-distributed, unpinned identity for `feature_id` in `environment` as
/// needing re-evaluation against current segment state - called whenever a segment's
/// rules/groups/overrides change in a way that could affect this feature. Resolution itself
/// is deferred: [`get_identity_variants`] re-evaluates and clears the flag the next time
/// each identity is actually read, rather than reconciling every affected identity eagerly.
pub(crate) async fn mark_feature_dirty(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
) -> anyhow::Result<()> {
    SQLIdentities::mark_feature_dirty(conn, params![feature_id, environment_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not mark feature dirty", e))?;

    Ok(())
}

/// Returns the variant_id currently assigned to the given identity for the given
/// feature+environment, or None if no assignment exists.
pub async fn get_variant_for_identity(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    identity: &Identity,
) -> anyhow::Result<Option<i32>> {
    SQLIdentities::fetch_identity_variant_for_feature::<_, (i32,)>(
        conn,
        params![identity.id, feature_id, environment.id],
    )
    .await
    .map(|id| id.map(|(i,)| i))
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity variant", e).into())
}

/// Returns the current variant assignments for an identity without triggering distribution.
/// Unassigned features are included with identity_id = None.
pub async fn list_variant_assignments(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &Identity,
) -> anyhow::Result<Vec<IdentityVariant>> {
    variant::get_by_identity(conn, environment, &identity.value).await
}

/// Evaluates which segment (if any) governs `feature_id` for `identity`, loading and
/// caching `identity`'s traits into `identity_traits` on first use (shared across every
/// feature resolved in one [`get_identity_variants`] call).
async fn evaluate_segment_for(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &Identity,
    identity_traits: &mut Option<Vec<IdentityTrait>>,
    feature_id: i32,
) -> anyhow::Result<Option<i32>> {
    if identity_traits.is_none() {
        *identity_traits = Some(load_traits(conn, identity.id).await?);
    }
    let ctx = evaluator::IdentityContext {
        value: &identity.value,
        traits: identity_traits.as_ref().unwrap(),
    };
    evaluator::evaluate(conn, environment, &ctx, feature_id).await
}

/// Returns enabled features variants assigned to given identity, distributing the
/// identity across variants as needed.
///
/// If the identity has a pending migration, it is re-attached to a variant determined
/// by the distributor and persisted for future requests.
pub async fn get_identity_variants(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &Identity,
) -> anyhow::Result<Vec<IdentityVariant>> {
    let mut tx = conn.begin().await?;
    let mut variants = variant::get_by_identity(&mut tx, environment, &identity.value).await?;

    // Only needed to evaluate segment rules; loaded at most once (on first use) and reused
    // across every feature in this call. Kept as just the traits (not a full
    // IdentityWithTraits) so evaluator::evaluate can borrow `identity.value` directly
    // instead of every caller cloning it.
    let mut identity_traits: Option<Vec<IdentityTrait>> = None;

    for var in variants.iter_mut() {
        // Resolve the variant (and the segment it's attributed to) to attach to: skip
        // pinned identities, follow a pending weight-migration if one exists, distribute
        // fresh for a never-assigned identity, or - for one flagged `segment_dirty` by a
        // segment change - re-evaluate and only actually redistribute if the attribution
        // genuinely changed (an unrelated segment's rule edit shouldn't reshuffle
        // identities it was never going to match).
        let attach_to_variant = if var.pinned_at.is_some() {
            None
        } else if let Some(id) = var.migrated_id {
            variant::get_by_id(&mut tx, environment, id, None)
                .await
                .ok()
                .map(|v| (v, None))
        } else if var.identity_id.is_none() {
            let segment_id = evaluate_segment_for(
                &mut tx,
                environment,
                identity,
                &mut identity_traits,
                var.feature_id,
            )
            .await?;
            let variant =
                distributor::distribute(&mut tx, environment, var.feature_id, segment_id).await?;
            Some((variant, segment_id))
        } else if var.segment_dirty {
            let segment_id = evaluate_segment_for(
                &mut tx,
                environment,
                identity,
                &mut identity_traits,
                var.feature_id,
            )
            .await?;
            if segment_id != var.segment_id {
                let variant = distributor::distribute(&mut tx, environment, var.feature_id, segment_id)
                    .await?;
                Some((variant, segment_id))
            } else {
                SQLIdentities::clear_identity_dirty(
                    &mut *tx,
                    params![identity.id, var.feature_id, environment.id],
                )
                .await
                .map_err(|e| FlagrantError::QueryFailed("Could not clear dirty flag", e))?;
                None
            }
        } else {
            None
        };

        if let Some((variant, segment_id)) = attach_to_variant {
            SQLIdentities::upsert_identity_variant(
                &mut *tx,
                params![
                    identity.id,
                    environment.id,
                    var.feature_id,
                    variant.id,
                    segment_id,
                    Option::<NaiveDateTime>::None
                ],
            )
            .await
            .map_err(|e| {
                FlagrantError::QueryFailed("Could not attach identity to given variant", e)
            })?;

            var.variant_id = Some(variant.id);
            var.feature_value = Some(variant.value);
        }
    }

    tx.commit().await?;
    Ok(variants)
}

/// Gradually redirects a percentage of identities currently evaluating to `from_variant_id`
/// so that they instead evaluate to `into_variant_id`, without touching identities that are
/// pinned (`pinned_at IS NOT NULL`).
///
/// This is a no-op unless `from_variant_id != into_variant_id`. It's called whenever a
/// variant's weight shifts (e.g. `variant::balance_control_weight`), so that only the delta
/// in weight causes identities to move rather than the whole cohort being re-distributed
/// and reshuffled.
///
/// Migration is lazy and cumulative: rather than rewriting `identity_variants.variant_id`
/// directly, the target is recorded in `identity_variants.migrated_id` (see the
/// `migrate_identities` SQL query). Readers resolve the effective variant by following
/// `migrated_id` when present, falling back to `variant_id` otherwise (see the resolution
/// logic in [`get_identity_variants`]). This means:
///
/// - Identities already migrated to `from_variant_id` (i.e. `migrated_id = from_variant_id`)
///   are eligible to be migrated onward to `into_variant_id`, continuing the chain.
/// - `by_percent` is a percentage of *all* identities in the environment (not just those
///   currently attached to `from_variant_id`), rounded up, so repeated calls with
///   increasing percentages progressively grow the migrated cohort - identities already
///   migrated are prioritized to keep their assignment stable, and are never migrated back.
/// - Only up to that many identities across the whole environment end up redirected to
///   `into_variant_id` this way; older attachments (by `attached_at`) are preferred when
///   picking who moves next, so migration is deterministic and repeatable.
///
/// Only touches organic (non-segment-governed) identities - segment-governed ones are
/// reconciled separately, lazily, via the `segment_dirty` flag (see [`mark_feature_dirty`]
/// and the resolution logic in [`get_identity_variants`]).
pub async fn migrate_identities(
    conn: &mut SqliteConnection,
    environment: &Environment,
    from_variant_id: i32,
    into_variant_id: i32,
    by_percent: u8,
) -> anyhow::Result<()> {
    if from_variant_id != into_variant_id {
        tracing::info!(from_variant_id, into_variant_id, "Migrating identities");
        SQLIdentities::migrate_identities(
            conn,
            params![environment.id, from_variant_id, into_variant_id, by_percent],
        )
        .await?;
    }
    Ok(())
}

/// Pins an identity to a specific variant for a given feature in the given environment,
/// bypassing normal variant distribution.
pub async fn override_variant(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &Identity,
    feature_id: i32,
    variant_id: i32,
) -> anyhow::Result<()> {
    SQLIdentities::upsert_identity_variant(
        conn,
        params![
            identity.id,
            environment.id,
            feature_id,
            variant_id,
            Option::<i32>::None,
            Some(Utc::now())
        ],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not override variant for identity", e))?;

    Ok(())
}

/// Returns the overrides (pinned identities) for a given feature as typed `FeatureOverride` values.
pub async fn list_overrides(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
) -> anyhow::Result<Vec<FeatureOverride>> {
    let rows = SQLIdentities::fetch_overrides_for_feature::<_, (String,)>(
        conn,
        params![environment_id, feature_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch overrides for feature", e))?;

    Ok(rows
        .into_iter()
        .map(|(s,)| FeatureOverride::Identity(s))
        .collect())
}

pub async fn detach_identities(
    conn: &mut SqliteConnection,
    from_variant_id: i32,
) -> anyhow::Result<()> {
    SQLIdentities::delete_attachments(conn, params![from_variant_id]).await?;
    Ok(())
}

// Internal helper: validates, upserts the trait by name, and links it to the identity
// with the given value. Shared by `attach_traits` and `patch`'s Add/SetValue handling.
async fn attach_trait(
    conn: &mut SqliteConnection,
    project_id: i32,
    identity_id: i32,
    name: String,
    value: Option<TraitValue>,
) -> anyhow::Result<()> {
    value.validate()?;

    let trait_rec = upsert(&mut *conn, project_id, name).await?;
    SQLIdentities::upsert_identity_trait(&mut *conn, params![identity_id, trait_rec.id, value])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not attach trait to identity", e))?;

    Ok(())
}
