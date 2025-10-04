use flagrant_types::{Environment, Feature, IdentityVariant};
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, SqliteConnection};

use crate::{distributor, errors::FlagrantError};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/identities.sql"]
struct SQLIdentities {}

#[derive(Debug, sqlx::FromRow)]
pub struct VariantIdentity {
    pub identity_id: i32,
    pub feature_id: i32,
    pub variant_id: i32,
    pub migrated_id: Option<i32>,
    pub identity: String,
}
pub async fn get_identities(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<Vec<VariantIdentity>> {
    let idents = SQLIdentities::fetch_identities(conn, params![environment.id, feature.id]).await?;
    Ok(idents)
}

pub async fn get_variants(
    conn: &mut SqliteConnection,
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
            // cache identity_id to avoid unnecessary query for same information later below
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
            var.value = variant.value;
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
