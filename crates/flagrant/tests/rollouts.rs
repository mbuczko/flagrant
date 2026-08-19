use flagrant::errors::FlagrantError;
use flagrant::models::{
    feature,
    identity::{self, HugSql, SQLIdentities},
    rollout, segment, snapshot, variant,
};
use flagrant_types::{
    Comparator, Environment, Feature, RolloutConfig, RolloutStep, Subject, VariantValue,
    payload::{FeaturePatch, RolloutPatchOp, SegmentPatchOp, SegmentVariantWeight, VariantPatchOp},
};
use hugsqlx::params;
use sqlx::{Sqlite, SqliteConnection, pool::PoolConnection};

use crate::common::{
    add_group, add_rule, apply, create_context, create_environment, create_feature,
};

mod common;

fn rollout_cfg(min_sample_size: u32, steps: &[(u8, Option<u32>)]) -> RolloutConfig {
    RolloutConfig {
        min_sample_size,
        steps: steps
            .iter()
            .map(|&(weight, hold_for_secs)| RolloutStep {
                weight,
                hold_for_secs,
            })
            .collect(),
    }
}

async fn set_rollout(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    cfg: RolloutConfig,
) -> anyhow::Result<Option<Feature>> {
    feature::patch(
        conn,
        environment,
        feature,
        FeaturePatch {
            rollout: Some(RolloutPatchOp::Set(cfg)),
            ..Default::default()
        },
    )
    .await
}

/// Pushes `feature_rollout_state.last_change_at` back by `seconds_ago`, so the next lazy
/// check (`identity::get_identity_variants`) sees a step's hold duration as elapsed -
/// mirrors how `tests/identities.rs`/`tests/segments.rs` directly manipulate DB state to
/// simulate the passage of time for a "resolved lazily on next read" mechanism.
async fn backdate(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
    seconds_ago: i64,
) {
    sqlx::query(
        "UPDATE feature_rollout_state SET last_change_at = datetime('now', ?) \
         WHERE feature_id = ? AND environment_id = ?",
    )
    .bind(format!("-{seconds_ago} seconds"))
    .bind(feature_id)
    .bind(environment_id)
    .execute(&mut *conn)
    .await
    .unwrap();
}

async fn rollout_state_rows_count(conn: &mut SqliteConnection, feature_id: i32) -> i64 {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM feature_rollout_state WHERE feature_id = ?")
            .bind(feature_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    count
}

async fn touch(conn: &mut SqliteConnection, environment: &Environment, value: &str) {
    let ident = identity::get_or_create_by_value(conn, environment, value.to_owned())
        .await
        .unwrap();
    identity::get_identity_variants(conn, environment, &ident)
        .await
        .unwrap();
}

#[derive(Debug, sqlx::FromRow)]
struct VariantMigration {
    #[allow(dead_code)]
    variant_id: i32,
    migrated_id: Option<i32>,
}

async fn migrations_count(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    migrated_to: Option<i32>,
) -> usize {
    let rows: Vec<VariantMigration> =
        SQLIdentities::fetch_identities(conn, params![environment.id, feature.id])
            .await
            .unwrap();

    rows.iter().filter(|r| r.migrated_id == migrated_to).count()
}

#[sqlx::test]
async fn enabling_rollout_requires_exactly_one_non_control_variant(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;

    // Zero non-control variants.
    let feature = create_feature(&mut conn, &environment, "no_alt").await;
    let err = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(100, None)]),
    )
    .await
    .unwrap_err();

    assert!(
        err.downcast_ref::<FlagrantError>()
            .is_some_and(|e| matches!(e, FlagrantError::BadRequest(_))),
        "expected BadRequest, got: {err}"
    );

    // Two non-control variants.
    let feature = create_feature(&mut conn, &environment, "two_alts").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("a"),
        30,
    )
    .await
    .unwrap();
    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("b"),
        30,
    )
    .await
    .unwrap();

    let err = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(100, None)]),
    )
    .await
    .unwrap_err();
    assert!(
        err.downcast_ref::<FlagrantError>()
            .is_some_and(|e| matches!(e, FlagrantError::BadRequest(_))),
        "expected BadRequest, got: {err}"
    );
}

#[sqlx::test]
async fn adding_second_variant_while_rollout_active_is_rejected(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        10,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(10, Some(3600)), (100, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    let patch = FeaturePatch {
        variants: vec![VariantPatchOp::Add {
            value: VariantValue::build("second"),
            weight: 10,
        }],
        ..Default::default()
    };
    let err = feature::patch(&mut conn, &environment, &feature, patch)
        .await
        .unwrap_err();

    assert!(
        err.downcast_ref::<FlagrantError>()
            .is_some_and(|e| matches!(e, FlagrantError::BadRequest(_))),
        "expected BadRequest, got: {err}"
    );
}

#[sqlx::test]
async fn enabling_rollout_applies_first_step_weight_immediately(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        10,
    )
    .await
    .unwrap();

    set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(50, Some(3600)), (100, None)]),
    )
    .await
    .unwrap();

    let alt = variant::get_by_id(&mut conn, &environment, alt.id, None)
        .await
        .unwrap();

    assert_eq!(
        alt.weight, 50,
        "enabling a rollout should immediately apply the first step's weight"
    );
}

#[sqlx::test]
async fn rollout_does_not_advance_below_minimum_sample_size(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        10,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(100, &[(10, Some(1)), (100, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "solo-identity").await;

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status.current_step, 0,
        "should not advance past step 0 with far fewer than 100 distributed identities"
    );

    let alt = variant::get_by_id(&mut conn, &environment, alt.id, None)
        .await
        .unwrap();

    assert_eq!(alt.weight, 10);
}

#[sqlx::test]
async fn rollout_catches_up_to_furthest_due_step_in_one_transition(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(10, Some(60)), (50, Some(60)), (100, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    let before = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap()
        .len();

    // Cumulative hold time to reach the terminal step is 120s - backdate well past that,
    // so a single lazy check should jump straight from step 0 to step 2.
    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "catch-up-identity").await;

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status.current_step, 2,
        "should jump straight to the terminal step"
    );

    let alt = variant::get_by_id(&mut conn, &environment, alt.id, None)
        .await
        .unwrap();
    assert_eq!(alt.weight, 100);

    let after = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap()
        .len();

    assert_eq!(
        after,
        before + 1,
        "catch-up should record exactly one new snapshot, not one per skipped step"
    );
}

#[sqlx::test]
async fn rollout_step_advance_reuses_migrate_identities(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(0, Some(60)), (50, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    // 10 identities, all organically distributed onto the control variant (alt is at 0%).
    for n in 1..=10 {
        touch(&mut conn, &environment, &format!("ident_{n}")).await;
    }
    assert_eq!(
        migrations_count(&mut conn, &environment, &feature, None).await,
        10
    );

    backdate(&mut conn, feature.id, environment.id, 1000).await;

    // Advancing is feature-wide, triggered by any read - use a fresh identity (not one of
    // the 10 being measured) so triggering the read doesn't also resolve one of their
    // migration pointers mid-assertion (a resolved identity shows as `variant_id = alt.id`
    // directly, not `migrated_id = Some(alt.id)`, which would silently undercount here).
    // This also nudges the environment's total identity count up by one, so the exact
    // rounding used by `migrate_identities`'s LIMIT isn't reproduced here - the point of
    // this test is only that the migration is partial (delta-based), not all-or-nothing.
    touch(&mut conn, &environment, "trigger-identity").await;

    // Weight jumped 0% -> 50%: roughly half the identities should be migrated to `alt`,
    // not all of them - the same delta-based mechanism exercised in
    // tests/identities.rs's `migrate_identities` test, reused here rather than reinvented.
    let migrated = migrations_count(&mut conn, &environment, &feature, Some(alt.id)).await;

    assert!(
        migrated > 0 && migrated < 10,
        "expected a partial migration of the 10 identities, got {migrated}"
    );
}

#[sqlx::test]
async fn rollout_step_advance_ignores_pinned_and_segment_governed_identities(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();
    let control_id = feature.get_default_variant().id;

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(0, Some(60)), (50, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    // Pin one identity explicitly to the control variant.
    let pinned =
        identity::get_or_create_by_value(&mut conn, &environment, "pinned-user".to_owned())
            .await
            .unwrap();

    identity::override_variant(&mut conn, &environment, &pinned, feature.id, control_id)
        .await
        .unwrap();

    // A segment overriding this feature to keep `alt` at 0% for anyone it governs.
    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "segment-user",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 0,
                }],
            },
        ],
    )
    .await;

    let segment_governed =
        identity::get_or_create_by_value(&mut conn, &environment, "segment-user".to_owned())
            .await
            .unwrap();

    touch(&mut conn, &environment, "segment-user").await;

    let attribution_before =
        identity::get_variant_for_identity(&mut conn, &environment, feature.id, &segment_governed)
            .await
            .unwrap();

    assert_eq!(attribution_before, Some(control_id));

    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "trigger-advance").await;

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status.current_step, 1,
        "sanity: the rollout should have advanced"
    );

    // Pinned identity: still on the control variant, untouched by the organic weight shift.
    let pinned_variant =
        identity::get_variant_for_identity(&mut conn, &environment, feature.id, &pinned)
            .await
            .unwrap();
    assert_eq!(pinned_variant, Some(control_id));

    // Segment-governed identity: the segment's own override (alt at 0%) is untouched by the
    // organic rollout, so it must still resolve to the control variant.
    let segment_variant =
        identity::get_variant_for_identity(&mut conn, &environment, feature.id, &segment_governed)
            .await
            .unwrap();

    assert_eq!(segment_variant, Some(control_id));
}

#[sqlx::test]
async fn rollout_step_advance_is_idempotent_on_repeated_reads(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(10, Some(60)), (100, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "first-read").await;

    let after_first = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(after_first.current_step, 1);

    let snaps_after_first = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap()
        .len();

    // Read again immediately - `last_change_at` was just reset to now, so nothing further
    // is due, and this must be a pure no-op: same step, no additional snapshot.
    touch(&mut conn, &environment, "second-read").await;

    let after_second = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(after_second.current_step, 1);

    let snaps_after_second = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap()
        .len();

    assert_eq!(snaps_after_second, snaps_after_first);
}

#[sqlx::test]
async fn disabling_rollout_clears_state_across_environments(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let env_b = create_environment(&mut conn, &project).await;

    let feature = create_feature(&mut conn, &environment, "prog").await;
    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        10,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(10, Some(3600)), (100, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    // Simulate the rollout having also been activated in a second environment.
    sqlx::query(
        "INSERT INTO feature_rollout_state(feature_id, environment_id, current_step, last_change_at) \
         VALUES (?, ?, 0, CURRENT_TIMESTAMP)",
    )
    .bind(feature.id)
    .bind(env_b.id)
    .execute(&mut *conn)
    .await
    .unwrap();

    assert_eq!(rollout_state_rows_count(&mut conn, feature.id).await, 2);

    let patch = FeaturePatch {
        rollout: Some(RolloutPatchOp::Unset),
        ..Default::default()
    };
    let updated = feature::patch(&mut conn, &environment, &feature, patch)
        .await
        .unwrap()
        .unwrap();

    assert!(updated.rollout.is_none());
    assert_eq!(
        rollout_state_rows_count(&mut conn, feature.id).await,
        0,
        "disabling should clear progression state across every environment"
    );
}

#[sqlx::test]
async fn restoring_a_snapshot_rolls_back_rollout_progression(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();

    let feature = set_rollout(
        &mut conn,
        &environment,
        &feature,
        rollout_cfg(0, &[(0, Some(60)), (50, None)]),
    )
    .await
    .unwrap()
    .unwrap();

    // Manually capture the pre-advance state (step 0), since only `commit::apply` (not a
    // direct `feature::patch` call, as used above) captures snapshots automatically.
    let v1 = snapshot::capture(
        &mut conn,
        &project,
        &environment,
        &feature,
        Some("before".into()),
    )
    .await
    .unwrap();

    assert_eq!(v1.version, 1);

    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "advance-trigger").await;

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status.current_step, 1,
        "sanity: rollout should have advanced"
    );

    let after_advance = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();

    snapshot::restore(&mut conn, &project, &environment, &after_advance, 1, None)
        .await
        .unwrap();

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status.current_step, 0,
        "restoring to v1 should roll back progression, not just weight"
    );
}

/// Regression test: a snapshot taken mid-progression under one schedule, restored *after*
/// the rules have since been replaced by a different schedule, must bring back the
/// original schedule too - not just re-apply a step index that's now meaningless against
/// whatever schedule happens to be live.
#[sqlx::test]
async fn restoring_a_snapshot_after_rules_changed_restores_matching_config(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "prog").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        0,
    )
    .await
    .unwrap();

    let v1_cfg = rollout_cfg(0, &[(10, Some(60)), (50, Some(60)), (100, None)]);
    let feature = set_rollout(&mut conn, &environment, &feature, v1_cfg.clone())
        .await
        .unwrap()
        .unwrap();

    // Catch up straight to the terminal step (100%) under v1 - this is a lazy advance, so
    // it captures its own snapshot automatically (see `rollout::maybe_advance`).
    backdate(&mut conn, feature.id, environment.id, 1000).await;
    touch(&mut conn, &environment, "advance-trigger").await;

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        status.current_step, 2,
        "sanity: should have caught up to v1's terminal step"
    );

    let snapshots = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap();
    let mid_progression_version = snapshots
        .iter()
        .map(|s| s.version)
        .max()
        .expect("the catch-up advance should have recorded a snapshot");

    // Now change the rules entirely - a shorter, differently-weighted schedule. This
    // resets progression to step 0 under v2, exactly as designed.
    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let v2_cfg = rollout_cfg(0, &[(20, Some(30)), (80, None)]);
    set_rollout(&mut conn, &environment, &feature, v2_cfg)
        .await
        .unwrap();

    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        status.current_step, 0,
        "sanity: changing rules resets to step 0"
    );
    assert_eq!(status.config.steps.len(), 2, "sanity: v2 is now live");

    // Restore the snapshot captured mid-progression under v1, while v2 is live.
    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    snapshot::restore(
        &mut conn,
        &project,
        &environment,
        &feature,
        mid_progression_version,
        None,
    )
    .await
    .unwrap();

    // The restored status must reflect v1's schedule again, not v2's - otherwise
    // `current_step: 2` would dangle against a 2-step (indices 0..1) v2 schedule.
    let status = rollout::get_status(&mut conn, &environment, feature.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        status.config.steps.len(),
        3,
        "restore must bring back v1's schedule, not leave v2 live"
    );
    assert_eq!(
        status.current_step, 2,
        "restored step must be valid against the restored (v1) schedule"
    );

    let alt = variant::get_by_id(&mut conn, &environment, alt.id, None)
        .await
        .unwrap();
    assert_eq!(
        alt.weight, 100,
        "restored weight should match v1's step 2 (100%), as captured in the snapshot"
    );
}
