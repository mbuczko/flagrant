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
    let mut variants = Vec::new();

    for mut var in variant::get_by_identity(&mut tx, environment, &identity).await? {
        // identity detached from a variant should be distributed to another one
        if var.is_detached {
            let variant = distributor::distribute(&mut tx, environment, var.feature_id).await?;

            Identities::upsert_identity(&mut *tx, params![&identity, variant.id])
                .await
                .map_err(|e| {
                    FlagrantError::QueryFailed("Could not attach identity to given variant", e)
                })?;

            var = IdentityVariant {
                feature_id: var.feature_id,
                variant_id: variant.id,
                value: variant.value,
                name: var.name,
                is_detached: false,
            };
        }
        variants.push(var);
    }
    Ok(variants)
}
