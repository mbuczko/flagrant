use chrono::{Duration, NaiveDateTime, Utc};
use flagrant_types::{Environment, Project, RolloutStatus};
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, SqliteConnection};

use super::{feature, snapshot, variant};
use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/rollouts.sql"]
struct SQLRollouts {}

#[derive(sqlx::FromRow)]
struct RolloutStateRow {
    #[allow(dead_code)]
    feature_id: i32,
    #[allow(dead_code)]
    environment_id: i32,
    current_step: i32,
    last_change_at: NaiveDateTime,
    #[allow(dead_code)]
    created_at: NaiveDateTime,
}

async fn fetch_state(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Option<RolloutStateRow>> {
    SQLRollouts::fetch_rollout_state::<_, RolloutStateRow>(
        conn,
        params![feature_id, environment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch rollout state", e).into())
}

/// Counts organically-distributed (non-pinned) identities for a (feature, environment)
/// pair - the input to the minimum-sample-size gate.
pub(crate) async fn count_distributed_identities(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<i64> {
    let (count,) = SQLRollouts::count_distributed_identities::<_, (i64,)>(
        conn,
        params![feature_id, environment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not count distributed identities", e))?;

    Ok(count)
}

/// Seeds/resets a feature's progression state back to step 0 for one environment. Called
/// by `feature::patch` when a rollout is enabled (`RolloutPatchOp::Set`).
pub(crate) async fn activate(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
) -> anyhow::Result<()> {
    SQLRollouts::activate_rollout(conn, params![feature_id, environment_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not activate progressive rollout", e))?;

    Ok(())
}

/// Returns the current step index for a feature's rollout in one environment, if the
/// feature has an active rollout there. Used by `snapshot::capture` to record enough
/// state for `snapshot::restore` to also roll back progression, not just weight.
pub(crate) async fn current_step(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Option<i32>> {
    Ok(fetch_state(conn, feature_id, environment_id)
        .await?
        .map(|s| s.current_step))
}

/// Force-sets progression state to an exact step. Used by `snapshot::restore`, which
/// restores to a specific historical step rather than resetting to 0 like [`activate`].
pub(crate) async fn set_step(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
    step: i32,
) -> anyhow::Result<()> {
    SQLRollouts::set_rollout_state(conn, params![feature_id, environment_id, step])
        .await
        .map_err(|e| {
            FlagrantError::QueryFailed("Could not restore progressive rollout state", e)
        })?;

    Ok(())
}

/// Clears progression state for a feature across every environment. Called by
/// `feature::patch` when a rollout is disabled (`RolloutPatchOp::Unset`) - state is
/// worthless once the rule list it depends on is gone.
pub(crate) async fn deactivate(conn: &mut SqliteConnection, feature_id: i32) -> anyhow::Result<()> {
    SQLRollouts::delete_rollout_state_for_feature(conn, params![feature_id])
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not clear progressive rollout state", e))?;

    Ok(())
}

/// Lazily advances `feature_id`'s progressive rollout (if any) in `environment` - called
/// once per unique feature touched by `identity::get_identity_variants`, before that
/// call's own resolution transaction opens.
///
/// No-op unless: the feature has `rollout` set, a `feature_rollout_state` row exists for
/// this environment (seeded by `feature::patch` on enable), and at least one step's hold
/// duration has elapsed since `last_change_at`.
///
/// Catch-up semantics: jumps straight to the furthest step whose *cumulative* hold time
/// (summed from the current step forward) is already covered by elapsed time - one
/// transition, one snapshot - never replays intermediate steps, since a replayed step
/// would falsely claim the feature spent real wall-clock time at a weight it never
/// actually held.
///
/// The minimum-sample-size gate is checked exactly once, immediately before the very
/// first timed advance (step 0 -> step 1) - never re-checked on later hops, so a rollout
/// won't stall mid-schedule if traffic later drops.
///
/// Applies a due transition as its own self-contained commit (CAS + weight shift + state
/// update + `snapshot::capture`), independent of the caller's transaction - mirroring how
/// `snapshot::restore` performs its own transactional commit+capture outside of
/// `commit::apply`. The CAS (`advance_rollout_state`) ensures that if two requests race to
/// advance the same due step, only one of them actually applies it.
///
/// Returns `true` if a transition was applied - callers must treat any already-fetched
/// `IdentityVariant`/`migrated_id` state for this feature as stale and re-fetch.
pub(crate) async fn maybe_advance(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<bool> {
    let current = feature::get_by_id(conn, environment, feature_id).await?;
    let Some(config) = current.rollout.clone() else {
        return Ok(false);
    };
    let Some(state) = fetch_state(conn, feature_id, environment.id).await? else {
        return Ok(false);
    };

    let step_count = config.steps.len();
    let current_step = state.current_step as usize;
    if current_step + 1 >= step_count {
        return Ok(false); // Already at the terminal step - nothing to advance to.
    }

    let now = Utc::now().naive_utc();
    let elapsed = now.signed_duration_since(state.last_change_at);

    let mut target = current_step;
    let mut cumulative = Duration::zero();
    for step in &config.steps[current_step..step_count - 1] {
        let Some(hold_for_secs) = step.hold_for_secs else {
            break; // Malformed config (validate_steps should prevent this) - stop safely.
        };
        cumulative += Duration::seconds(hold_for_secs as i64);
        if cumulative > elapsed {
            break;
        }
        target += 1;
    }

    if target == current_step {
        return Ok(false); // Nothing due yet.
    }

    if current_step == 0 {
        let sample_size = count_distributed_identities(conn, feature_id, environment.id).await?;
        if sample_size < config.min_sample_size as i64 {
            return Ok(false);
        }
    }

    let mut tx = conn.begin().await?;

    let advanced = SQLRollouts::advance_rollout_state::<_, RolloutStateRow>(
        &mut *tx,
        params![
            feature_id,
            environment.id,
            target as i32,
            now,
            state.current_step
        ],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not advance rollout state", e))?;

    if advanced.is_none() {
        // Another request already won the race and advanced this rollout first.
        return Ok(false);
    }

    let Some(alternative) = current.variants.iter().find(|v| !v.is_control()) else {
        tracing::warn!(
            feature_id,
            "Progressive rollout has no alternative variant to advance"
        );
        return Ok(false);
    };

    variant::set_weight_and_rebalance(
        &mut tx,
        environment,
        feature_id,
        alternative,
        config.steps[target].weight,
    )
    .await?;

    let project = Project {
        id: environment.project_id,
        ..Default::default()
    };
    let updated = feature::get_by_id(&mut tx, environment, feature_id).await?;
    let comment = format!(
        "progressive rollout: step {} ({}%)",
        target + 1,
        config.steps[target].weight
    );
    snapshot::capture(&mut tx, &project, environment, &updated, Some(comment)).await?;

    tx.commit().await?;
    Ok(true)
}

/// Read-only live view of a feature's progressive rollout in one environment - the
/// schedule plus where it currently stands. Never advances the schedule (a `GET` must not
/// mutate) - use [`maybe_advance`] for that.
pub async fn get_status(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<Option<RolloutStatus>> {
    let feature = feature::get_by_id(conn, environment, feature_id).await?;
    let Some(config) = feature.rollout else {
        return Ok(None);
    };
    let Some(state) = fetch_state(conn, feature_id, environment.id).await? else {
        return Ok(None);
    };
    let distributed_identities =
        count_distributed_identities(conn, feature_id, environment.id).await?;

    Ok(Some(RolloutStatus {
        config,
        current_step: state.current_step,
        last_change_at: state.last_change_at,
        distributed_identities,
    }))
}
