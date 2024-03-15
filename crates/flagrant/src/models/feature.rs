use hugsqlx::{params, HugSqlx};
use sqlx::{Pool, Sqlite};

use flagrant_types::{Feature, Project};
use crate::errors::DbError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/features.sql"]
struct Features {}

pub async fn create(pool: &Pool<Sqlite>, project: &Project, name: String, value: String, is_enabled: bool) -> anyhow::Result<Feature> {
    Ok(
        Features::create_feature::<_, Feature>(pool, params!(project.id, name, value, is_enabled))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not create a feature");
                DbError::QueryFailed
            })?
    )
}

pub async fn fetch(pool: &Pool<Sqlite>, feature_id: u16) -> anyhow::Result<Feature> {
    Ok(
        Features::fetch_feature::<_, Feature>(pool, params!(feature_id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch a feature");
                DbError::QueryFailed
            })?
    )
}

pub async fn list(pool: &Pool<Sqlite>, project: &Project) -> anyhow::Result<Vec<Feature>> {
    Ok(
        Features::fetch_features_for_project::<_, Feature>(pool, params!(project.id))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Could not fetch features for project");
                DbError::QueryFailed
            })?
    )
}

