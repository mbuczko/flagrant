use chrono::{NaiveDateTime, Utc};
use flagrant_types::payload::{IdentityPatch, IdentityTraitPayload, TraitPatchOp};
use flagrant_types::{
    Environment, FeatureOverride, FeatureValue, Identity, IdentityTrait, IdentityVariant,
    IdentityWithTraits,
};

use super::feature;
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, SqliteConnection};

use crate::{distributor, errors::FlagrantError};

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

// Helper to load traits for an identity_id
async fn load_traits(
    conn: &mut SqliteConnection,
    identity_id: i32,
) -> anyhow::Result<Vec<IdentityTrait>> {
    SQLIdentities::fetch_identity_traits::<_, IdentityTrait>(conn, params![identity_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity traits", e).into())
}

/// Lists up to 10 identities with their traits, optionally filtered by pattern
pub async fn list(
    conn: &mut SqliteConnection,
    environment: &Environment,
    pattern: Option<String>,
) -> anyhow::Result<Vec<IdentityWithTraits>> {
    let like = pattern.unwrap_or_else(|| "%".to_string());
    let rows = SQLIdentities::fetch_identities_with_traits::<_, IdentityWithTraitRow>(
        conn,
        params![environment.project_id, environment.id, like],
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

    attach_traits(&mut tx, environment, &identity, &trait_payloads).await?;
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
    attach_traits(&mut tx, environment, &identity, &trait_payloads).await?;

    tx.commit().await?;
    get_by_value_with_traits(conn, environment, identity.value).await
}

/// Applies a patch to an identity — applies granular trait operations
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
                let trait_rec = upsert(&mut tx, environment.project_id, name).await?;
                SQLIdentities::upsert_identity_trait(
                    &mut *tx,
                    params![identity.id, trait_rec.id, value],
                )
                .await
                .map_err(|e| FlagrantError::QueryFailed("Could not attach trait to identity", e))?;
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

    for var in variants.iter_mut() {
        // Resolve the variant to attach to: skip pinned identities, follow a pending
        // migration if one exists, or distribute a fresh identity for the first time.
        let attach_to_variant = if var.pinned_at.is_some() {
            None
        } else if let Some(id) = var.migrated_id {
            variant::get_by_id(&mut tx, environment, id, None)
                .await
                .ok()
        } else if var.identity_id.is_none() {
            // TODO: resolve the identity's matching segment via the rule evaluator once
            // it exists, and pass its id here instead of None.
            Some(distributor::distribute(&mut tx, environment, var.feature_id, None).await?)
        } else {
            None
        };

        if let Some(variant) = attach_to_variant {
            SQLIdentities::upsert_identity_variant(
                &mut *tx,
                params![
                    identity.id,
                    environment.id,
                    var.feature_id,
                    variant.id,
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

// Internal helper: upserts traits and links them to identity
async fn attach_traits(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &Identity,
    trait_payloads: &[IdentityTraitPayload],
) -> anyhow::Result<()> {
    for t in trait_payloads {
        let trait_rec = upsert(&mut *conn, environment.project_id, t.name.clone()).await?;
        SQLIdentities::upsert_identity_trait(
            &mut *conn,
            params![identity.id, trait_rec.id, t.value.clone()],
        )
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not attach trait to identity", e))?;
    }
    Ok(())
}
