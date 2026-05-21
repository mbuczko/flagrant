use flagrant_types::payload::{IdentityPatch, IdentityTraitPayload, TraitPatchOp};
use flagrant_types::{
    Environment, Identity, IdentityTrait, IdentityVariant, IdentityWithTraits, Project,
};
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

/// Lists up to 10 identities with their traits, optionally filtered by pattern.
pub async fn list(
    conn: &mut SqliteConnection,
    project: &Project,
    pattern: Option<String>,
) -> anyhow::Result<Vec<IdentityWithTraits>> {
    let like = pattern
        .map(|p| format!("%{p}%"))
        .unwrap_or_else(|| "%".to_string());
    let rows = SQLIdentities::fetch_identities_with_traits::<_, IdentityWithTraitRow>(
        conn,
        params![project.id, like],
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

/// Fetches a single identity with no traits by project and value
pub async fn get_by_value(
    conn: &mut SqliteConnection,
    project: &Project,
    value: String,
) -> anyhow::Result<Identity> {
    let (id, val) = SQLIdentities::fetch_identity_by_value::<_, (i32, String)>(
        &mut *conn,
        params![project.id, value],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity", e))?;

    Ok(Identity { id, value: val })
}

/// Fetches a single identity with its traits by identity value.
pub async fn get_with_traits(
    conn: &mut SqliteConnection,
    project: &Project,
    value: String,
) -> anyhow::Result<IdentityWithTraits> {
    let (id, identity) = SQLIdentities::fetch_identity_by_value::<_, (i32, String)>(
        &mut *conn,
        params![project.id, value],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch identity", e))?;

    Ok(IdentityWithTraits {
        id,
        value: identity,
        traits: load_traits(conn, id).await?,
    })
}

/// Creates a new identity with optional traits. Auto-creates traits that don't exist yet.
pub async fn create(
    conn: &mut SqliteConnection,
    project: &Project,
    identity: String,
    trait_payloads: Vec<IdentityTraitPayload>,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;
    let (id, value) =
        SQLIdentities::upsert_identity::<_, (i32, String)>(&mut *tx, params![project.id, identity])
            .await?;

    attach_traits(&mut *tx, project, id, &trait_payloads).await?;
    tx.commit().await?;

    let traits = load_traits(conn, id).await?;
    Ok(IdentityWithTraits { id, value, traits })
}

/// Replaces all traits for an identity. Auto-creates traits that don't exist yet.
pub async fn update_traits(
    conn: &mut SqliteConnection,
    project: &Project,
    identity: Identity,
    trait_payloads: Vec<IdentityTraitPayload>,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;

    SQLIdentities::delete_identity_traits(&mut *tx, params![identity.id]).await?;
    attach_traits(&mut *tx, project, identity.id, &trait_payloads).await?;

    tx.commit().await?;
    get_with_traits(conn, project, identity.value).await
}

/// Applies a patch to an identity — optionally renames it and applies granular trait operations.
pub async fn patch(
    conn: &mut SqliteConnection,
    project: &Project,
    identity: Identity,
    patch: IdentityPatch,
) -> anyhow::Result<IdentityWithTraits> {
    let mut tx = conn.begin().await?;

    if let Some(new_value) = patch.identity {
        SQLIdentities::update_identity(&mut *tx, params![new_value, identity.id])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not update identity value", e))?;
    }

    for op in patch.traits {
        match op {
            TraitPatchOp::Add { name, value } | TraitPatchOp::SetValue { name, value } => {
                let trait_rec = upsert(&mut *tx, project, name).await?;
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
                    params![identity.id, project.id, name],
                )
                .await
                .map_err(|e| {
                    FlagrantError::QueryFailed("Could not delete trait from identity", e)
                })?;
            }
        }
    }

    tx.commit().await?;
    get_with_traits(conn, project, identity.value).await
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

// Internal helper: upserts traits and links them to identity
async fn attach_traits(
    conn: &mut SqliteConnection,
    project: &Project,
    identity_id: i32,
    trait_payloads: &[IdentityTraitPayload],
) -> anyhow::Result<()> {
    for t in trait_payloads {
        let trait_rec = upsert(&mut *conn, project, t.name.clone()).await?;
        SQLIdentities::upsert_identity_trait(
            &mut *conn,
            params![identity_id, trait_rec.id, t.value.clone()],
        )
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not attach trait to identity", e))?;
    }
    Ok(())
}

/// Returns feature variants assigned to given identity, distributing the identity across
/// variants as needed. If the identity is new or has a pending migration, it is attached
/// to a variant determined by the distributor and persisted for future requests.
pub async fn get_identity_variants(
    conn: &mut SqliteConnection,
    project: &Project,
    environment: &Environment,
    identity: String,
) -> anyhow::Result<Vec<IdentityVariant>> {
    let mut tx = conn.begin().await?;
    let mut variants = variant::get_by_identity(&mut tx, environment, &identity).await?;
    let mut identity_id: Option<i32> = None;

    for var in variants.iter_mut() {
        let attach_to_variant = if let Some(id) = var.migrated_id {
            variant::get_by_id(&mut tx, environment, id).await.ok()
        } else if var.identity_id.is_none() {
            Some(distributor::distribute(&mut tx, environment, var.feature_id).await?)
        } else {
            // Cache identity_id to avoid an unnecessary query for the same information later
            identity_id = var.identity_id;
            None
        };

        if let Some(variant) = attach_to_variant {
            if identity_id.is_none() {
                let (id, _) = SQLIdentities::upsert_identity::<_, (i32, String)>(
                    &mut *tx,
                    params![project.id, &identity],
                )
                .await?;
                identity_id = Some(id);
            }
            SQLIdentities::upsert_identity_variant(
                &mut *tx,
                params![
                    identity_id.unwrap(),
                    environment.id,
                    var.feature_id,
                    variant.id
                ],
            )
            .await
            .map_err(|e| {
                FlagrantError::QueryFailed("Could not attach identity to given variant", e)
            })?;

            var.variant_id = variant.id;
            var.feature_value = variant.value;
        }
    }

    tx.commit().await?;
    Ok(variants)
}

pub async fn migrate_identities(
    conn: &mut SqliteConnection,
    environment: &Environment,
    from_variant_id: i32,
    to_variant_id: i32,
    by_percent: u8,
) -> anyhow::Result<()> {
    if from_variant_id != to_variant_id {
        tracing::info!(from_variant_id, to_variant_id, "Migrating identities");
        SQLIdentities::migrate_identities(
            conn,
            params![environment.id, from_variant_id, to_variant_id, by_percent],
        )
        .await?;
    }
    Ok(())
}

pub async fn detach_identities(
    conn: &mut SqliteConnection,
    from_variant_id: i32,
) -> anyhow::Result<()> {
    SQLIdentities::delete_attachments(conn, params![from_variant_id]).await?;
    Ok(())
}
