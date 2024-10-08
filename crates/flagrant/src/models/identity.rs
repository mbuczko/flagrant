use flagrant_types::{Environment, IdentityVariant};
use hugsqlx::{params, HugSqlx};
use sqlx::{Connection, SqliteConnection};

use crate::{distributor, errors::FlagrantError};

use super::variant;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/identities.sql"]
struct Identities {}

pub async fn get_variants(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: String,
) -> anyhow::Result<Vec<IdentityVariant>> {
    let mut tx = conn.begin().await?;
    let mut variants = variant::get_by_identity(&mut tx, environment, &identity).await?;
    let mut identity_id: Option<i32> = None;

    for var in variants.iter_mut() {
        if let Some(id) = var.identity_id {
            identity_id = Some(id);
        }

        // if identity is detached from a variant or hasn't been attached to feature yet
        // it should be re/attached to feature variant chosen by distributor.
        if var.is_detached || var.identity_id.is_none() {
            let variant = distributor::distribute(&mut tx, environment, var.feature_id).await?;

            if identity_id.is_none() {
                let (id, _) =
                    Identities::upsert_identity::<_, (i32, String)>(&mut *tx, params![&identity])
                        .await?;
                identity_id = Some(id);
            }

            println!(
                "identity: {:?}, env: {}, feature: {}, variant: {}",
                identity_id, environment.id, var.feature_id, variant.id
            );

            Identities::upsert_identity_variant(
                &mut *tx,
                params![identity_id, environment.id, var.feature_id, variant.id],
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

pub async fn reconcile_attached_identities(
    conn: &mut SqliteConnection,
    environment: &Environment,
    variant_id: i32,
    weight: u8,
) -> anyhow::Result<()> {
    Identities::reset_detached_identities(&mut *conn, params![environment.id, variant_id]).await?;
    Identities::detach_identities_from_variant(
        &mut *conn,
        params![environment.id, variant_id, weight],
    )
    .await?;
    Ok(())
}
