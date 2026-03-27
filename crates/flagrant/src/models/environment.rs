use anyhow::bail;
use hugsqlx::{HugSqlx, params};
use sqlx::{Acquire, SqliteConnection};

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Project};

use super::{feature, variant};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/environments.sql"]
struct SQLEnvironments {}

/// Creates a new environment and optionally clones variants from a base environment.
///
/// `base_env_id` is optional for the first two environments in a project (there is nothing
/// meaningful to inherit from), but required for every subsequent one. When provided, all
/// features in the project will have their control variant value and non-control variant
/// weights copied from `base_env_id` into the new environment.
pub async fn create(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
    description: Option<String>,
    base_env_id: Option<i32>,
) -> anyhow::Result<Environment> {
    let existing = get_by_project(conn, project).await?;
    if existing.len() >= 2 && base_env_id.is_none() {
        bail!("base_env_id is required when creating a third or later environment");
    }

    let env =
        SQLEnvironments::create_environment(&mut *conn, params![project.id, name, description])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not create an environment", e))?;

    if let Some(base_env_id) = base_env_id {
        let base_env = get_by_id(conn, base_env_id).await?;
        clone_variants_from_env(conn, &base_env, &env).await?;
    }

    Ok(env)
}

/// Copies all feature variants from `base_env` into `new_env`.
///
/// For each feature in the project the function:
/// 1. Creates a control variant in `new_env` with the same value as in `base_env`.
/// 2. Inserts weight entries for every non-control variant using the weights from `base_env`.
/// 3. Recalculates the control variant weight so that all weights still sum to 100.
async fn clone_variants_from_env(
    conn: &mut SqliteConnection,
    base_env: &Environment,
    new_env: &Environment,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let features = feature::get_all(&mut tx, base_env, None, None, None, None, None).await?;

    for feat in &features {
        let control_value = feat.get_default_value().clone();
        variant::create_control(&mut tx, new_env, feat, control_value).await?;

        // Fetch all variants (including non-control) from base_env for this feature.
        let all_variants = variant::get_all(&mut tx, base_env, feat.id)
            .await
            .unwrap_or_default();

        let non_control: Vec<_> = all_variants.iter().filter(|v| !v.is_control()).collect();
        for v in &non_control {
            variant::set_weight(&mut tx, new_env, v.id, v.weight).await?;
        }
        if !non_control.is_empty() {
            variant::recalculate_control_weight(&mut tx, new_env, feat.id).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn get_by_id(
    conn: &mut SqliteConnection,
    environment_id: i32,
) -> anyhow::Result<Environment> {
    let env = SQLEnvironments::fetch_environment(conn, params![environment_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch environment", e))?;

    Ok(env)
}

pub async fn get_by_name(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
) -> anyhow::Result<Environment> {
    let env = SQLEnvironments::fetch_environment_by_name(conn, params![project.id, name])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch environment", e))?;

    Ok(env)
}

pub async fn get_by_prefix(
    conn: &mut SqliteConnection,
    project: &Project,
    prefix: String,
) -> anyhow::Result<Vec<Environment>> {
    let envs = SQLEnvironments::fetch_environments_by_pattern::<_, Environment>(
        conn,
        params![project.id, format!("{}%", prefix)],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of environments", e))?;

    Ok(envs)
}

pub async fn get_by_project(
    conn: &mut SqliteConnection,
    project: &Project,
) -> anyhow::Result<Vec<Environment>> {
    let envs = SQLEnvironments::fetch_environments_for_project(conn, params![project.id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of environments", e))?;

    Ok(envs)
}
