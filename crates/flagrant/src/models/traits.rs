use flagrant_types::{Project, Trait};
use hugsqlx::{HugSqlx, params};
use sqlx::{Acquire, SqliteConnection};

use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/traits.sql"]
struct SQLTraits {}

/// Creates a new trait or returns it if one with the same name already exists.
pub async fn upsert(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
) -> anyhow::Result<Trait> {
    let t = SQLTraits::upsert_trait::<_, Trait>(conn, params![project.id, name]).await?;
    Ok(t)
}

/// Returns all traits ordered by name.
pub async fn get_by_id(
    conn: &mut SqliteConnection,
    project: &Project,
    trait_id: i32,
) -> anyhow::Result<Trait> {
    let t = SQLTraits::fetch_trait_by_id::<_, Trait>(conn, params![project.id, trait_id]).await?;
    Ok(t)
}

/// Returns all traits ordered by name.
pub async fn get_all(conn: &mut SqliteConnection, project: &Project) -> anyhow::Result<Vec<Trait>> {
    let traits = SQLTraits::fetch_all_traits::<_, Trait>(conn, params![project.id]).await?;
    Ok(traits)
}

/// Deletes a trait and removes it from all identities.
pub async fn delete(
    conn: &mut SqliteConnection,
    project: &Project,
    trait_id: i32,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;

    if let Ok(_) = get_by_id(&mut *tx, project, trait_id).await {
        SQLTraits::delete_trait_entries(&mut *tx, params![trait_id]).await?;
        SQLTraits::delete_trait(&mut *tx, params![trait_id]).await?;

        tx.commit().await?;
        return Ok(());
    }
    Err(FlagrantError::NotFound("Could not find trait of given id").into())
}
