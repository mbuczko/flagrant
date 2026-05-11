use flagrant_types::{Environment, Identity, IdentityVariant};
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, SqliteConnection};

use crate::{distributor, errors::FlagrantError};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/identities.sql"]
pub struct SQLIdentities {}

pub async fn get_variants(
    conn: &mut SqliteConnection,
    environment: &Environment,
    Identity(identity): Identity,
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
                    params![&identity],
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
