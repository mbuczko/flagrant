use flagrant_types::Trait;
use hugsqlx::{HugSqlx, params};
use sqlx::SqliteConnection;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/traits.sql"]
struct SQLTraits {}

/// Creates a new trait or returns it if one with the same name already exists.
pub async fn upsert(conn: &mut SqliteConnection, name: String) -> anyhow::Result<Trait> {
    let t = SQLTraits::upsert_trait::<_, Trait>(conn, params![name]).await?;
    Ok(t)
}

/// Returns all traits ordered by name.
pub async fn get_all(conn: &mut SqliteConnection) -> anyhow::Result<Vec<Trait>> {
    let traits = SQLTraits::fetch_all_traits::<_, Trait>(conn, params![]).await?;
    Ok(traits)
}

/// Deletes a trait and removes it from all identities.
pub async fn delete(conn: &mut SqliteConnection, trait_id: i32) -> anyhow::Result<()> {
    SQLTraits::delete_trait_entries(&mut *conn, params![trait_id]).await?;
    SQLTraits::delete_trait(&mut *conn, params![trait_id]).await?;
    Ok(())
}
