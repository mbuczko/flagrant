use flagrant_types::Trait;
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use sqlx::{Acquire, SqliteConnection};

use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/traits.sql"]
struct SQLTraits {}

/// Creates a new trait or returns it if one with the same name already exists.
///
/// Runs in its own transaction so an invalid name is rolled back rather than committed:
/// the row is written first, then validated, and only committed if validation passes.
pub async fn upsert(
    conn: &mut SqliteConnection,
    project_id: i32,
    name: String,
) -> anyhow::Result<Trait> {
    let mut tx = conn.begin().await?;
    let t = SQLTraits::upsert_trait::<_, Trait>(&mut *tx, params![project_id, name]).await?;

    t.validate()?;
    tx.commit().await?;

    Ok(t)
}

/// Returns all traits ordered by name.
pub async fn get_by_id(conn: &mut SqliteConnection, trait_id: i32) -> anyhow::Result<Trait> {
    let t = SQLTraits::fetch_trait_by_id::<_, Trait>(conn, params![trait_id]).await?;
    Ok(t)
}

/// Returns all traits ordered by name.
pub async fn get_all(conn: &mut SqliteConnection, project_id: i32) -> anyhow::Result<Vec<Trait>> {
    let traits = SQLTraits::fetch_all_traits::<_, Trait>(conn, params![project_id]).await?;

    Ok(traits)
}

/// Returns traits with names matching the given LIKE pattern.
pub async fn get_by_prefix(
    conn: &mut SqliteConnection,
    project_id: i32,
    pattern: String,
) -> anyhow::Result<Vec<Trait>> {
    let traits =
        SQLTraits::fetch_traits_by_prefix::<_, Trait>(conn, params![project_id, pattern]).await?;

    Ok(traits)
}

/// Deletes a trait and removes it from all identities.
pub async fn delete(conn: &mut SqliteConnection, trait_id: i32) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    if get_by_id(&mut tx, trait_id).await.is_ok() {
        SQLTraits::delete_trait_entries(&mut *tx, params![trait_id]).await?;
        SQLTraits::delete_trait(&mut *tx, params![trait_id]).await?;

        tx.commit().await?;
        return Ok(());
    }
    Err(FlagrantError::NotFound("Could not find trait of given id").into())
}
