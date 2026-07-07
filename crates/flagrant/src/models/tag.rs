use flagrant_types::{Environment, Tag};
use hugsqlx::{HugSqlx, params};
use sqlx::{Row, SqliteConnection};

use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/tags.sql"]
struct SQLTags {}

pub async fn get_by_prefix(
    conn: &mut SqliteConnection,
    environment: &Environment,
    prefix: String,
) -> anyhow::Result<Vec<Tag>> {
    let tags = SQLTags::fetch_tags_by_pattern(
        conn,
        params![environment.project_id, format!("{prefix}%")],
        |row| Tag {
            name: row.get("tag"),
        },
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch tags", e))?;

    Ok(tags)
}

pub async fn get_all(
    conn: &mut SqliteConnection,
    environment: &Environment,
) -> anyhow::Result<Vec<Tag>> {
    get_by_prefix(conn, environment, String::default()).await
}
