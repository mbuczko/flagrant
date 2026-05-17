use anyhow::bail;
use hugsqlx::{HugSqlx, params};
use serde_valid::Validate;
use sqlx::{Acquire, SqliteConnection};

use crate::errors::FlagrantError;
use flagrant_types::{Environment, Project};

use super::{feature, variant};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/environments.sql"]
struct SQLEnvironments {}

/// Creates a new environment, inheriting feature variants from a base environment.
///
/// The base environment is resolved as follows:
/// - If `base_env` name is provided, it is used explicitly.
/// - If there is exactly one existing environment, it is used automatically.
/// - If there are two or more existing environments and no `base_env` is given, an error
///   is returned — the caller must be explicit about which environment to inherit from.
/// - On the very first environment in a project there is nothing to inherit, so no
///   cloning takes place.
///
/// When a base is resolved, all features will have their control variant value and
/// non-control variant weights copied from the base into the new environment.
pub async fn create(
    conn: &mut SqliteConnection,
    project: &Project,
    name: String,
    description: Option<String>,
    base_env: Option<String>,
) -> anyhow::Result<Environment> {
    let mut tx = conn.begin().await?;
    let existing = get_by_project(&mut tx, project).await?;

    let base = match (base_env, existing.len()) {
        (Some(name), _) => Some(get_by_name(&mut tx, project, name).await?),
        (None, 1) => existing.into_iter().next(),
        (None, n) if n >= 2 => {
            bail!("base_env is required when creating a third or later environment")
        }
        _ => None,
    };

    let env: Environment =
        SQLEnvironments::create_environment(&mut *tx, params![project.id, name, description])
            .await
            .map_err(|e| FlagrantError::QueryFailed("Could not create an environment", e))?;

    env.validate()?;

    if let Some(base) = base {
        clone_variants_from_env(&mut tx, &base, &env).await?;
    }

    tx.commit().await?;
    Ok(env)
}

/// Copies all feature variants from `base_env` into `new_env`.
///
/// A newly created environment has no variant data of its own, so without cloning every
/// feature would appear value-less and all traffic would fall back to undefined behaviour.
/// By inheriting from `base_env` the new environment starts in a known, valid state -
/// identical distribution weights and control values that can tuned lated independently
/// without affecting other environments.
///
/// The entire operation runs inside a single transaction so that a feature added concurrently
/// between the snapshot read and the writes does not produce a partially-cloned environment.
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
        let all_variants = variant::get_for_feature(&mut tx, base_env, feat.id)
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

pub async fn list(
    conn: &mut SqliteConnection,
    project: &Project,
    pattern: Option<String>,
) -> anyhow::Result<Vec<Environment>> {
    let envs = match pattern {
        Some(p) => {
            SQLEnvironments::fetch_environments_by_pattern::<_, Environment>(
                conn,
                params![project.id, p],
            )
            .await
        }
        None => {
            SQLEnvironments::fetch_environments_for_project(conn, params![project.id]).await
        }
    }
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch list of environments", e))?;

    Ok(envs)
}

pub async fn get_by_project(
    conn: &mut SqliteConnection,
    project: &Project,
) -> anyhow::Result<Vec<Environment>> {
    list(conn, project, None).await
}
