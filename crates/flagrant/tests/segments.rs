use std::collections::HashMap;

use flagrant::{
    distributor,
    models::{
        identity::{self, HugSql, SQLIdentities},
        rule, segment, variant,
    },
};
use flagrant_types::{
    Comparator, Environment, Feature, FeatureValue, Identity, Subject,
    payload::{SegmentPatch, SegmentPatchOp, SegmentVariantWeight},
};
use hugsqlx::params;
use sqlx::{Sqlite, SqliteConnection, pool::PoolConnection};

use crate::common::{add_group, add_rule, apply, create_context, create_feature};

mod common;

#[derive(Debug, sqlx::FromRow)]
struct IdentityAttribution {
    identity_id: i32,
    variant_id: i32,
    migrated_id: Option<i32>,
    segment_id: Option<i32>,
    segment_dirty: bool,
}

async fn attribution_for(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
    identity_id: i32,
) -> IdentityAttribution {
    let rows: Vec<IdentityAttribution> =
        SQLIdentities::fetch_identities(conn, params![environment.id, feature.id])
            .await
            .unwrap();
    rows.into_iter()
        .find(|r| r.identity_id == identity_id)
        .unwrap()
}

/// Forces resolution of any dirty/unassigned rows for `ident` by simulating the real
/// per-request read path (`GET .../features`, backed by `get_identity_variants`).
async fn resolve(conn: &mut SqliteConnection, environment: &Environment, ident: &Identity) {
    identity::get_identity_variants(conn, environment, ident)
        .await
        .unwrap();
}

/// A segment's `SetFeatureOverride` should write its explicit weights plus a control-variant
/// remainder straight into `variant_weights`, scoped by `segment_id`, without touching the
/// organic (segment_id = NULL) weights at all.
#[sqlx::test]
async fn segment_override_writes_into_variant_weights_alongside_organic_weights(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;

    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    let patch = SegmentPatch {
        ops: vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 30,
            }],
        }],
    };
    segment::patch(&mut conn, &project, segment.clone(), patch)
        .await
        .unwrap();

    // Organic (segment_id = NULL) weights are untouched: control=60, alt=40.
    let organic = variant::get_for_feature(&mut conn, &environment, feature.id, None)
        .await
        .unwrap();
    let organic_alt = organic.iter().find(|v| v.id == alt.id).unwrap();
    assert_eq!(organic_alt.weight, 40);

    let organic_control = organic.iter().find(|v| v.is_control()).unwrap();
    assert_eq!(organic_control.weight, 60);

    // Segment-scoped weights: alt=30 (explicit), control=70 (auto-balanced remainder).
    let scoped = variant::get_for_feature(&mut conn, &environment, feature.id, Some(segment.id))
        .await
        .unwrap();
    let scoped_alt = scoped.iter().find(|v| v.id == alt.id).unwrap();
    assert_eq!(scoped_alt.weight, 30);

    let scoped_control = scoped.iter().find(|v| v.is_control()).unwrap();
    assert_eq!(scoped_control.weight, 70);

    // get_segment_weights (backs the editor prefill) only surfaces the explicit override,
    // not the control variant's auto-balanced remainder.
    let overrides = variant::get_segment_weights(&mut conn, segment.id, feature.id, environment.id)
        .await
        .unwrap();
    assert_eq!(overrides, vec![(alt.id, 30)]);

    // list_overrides_for_feature (backs "FEATURE describe") includes the control variant's
    // remainder too, so users can see where the rest of the percentages go.
    let control = organic.iter().find(|v| v.is_control()).unwrap();
    let displayed = segment::list_overrides_for_feature(&mut conn, environment.id, feature.id)
        .await
        .unwrap();
    let (_, _, weights) = displayed.iter().find(|(_, name, _)| name == "vip").unwrap();
    let mut by_id: Vec<(i32, u8)> = weights.iter().map(|w| (w.variant_id, w.weight)).collect();
    let mut expected = vec![(alt.id, 30), (control.id, 70)];

    by_id.sort();
    expected.sort();

    assert_eq!(by_id, expected);
}

/// Creating a new variant for a feature that's already segment-overridden should seed that
/// variant into the overriding segment at 0% weight - materializing it there (so it shows
/// up in listings/the editor) without disturbing the segment's control-variant remainder,
/// since a 0-weight row doesn't change the sum it's computed from.
#[sqlx::test]
async fn creating_a_variant_seeds_it_into_overriding_segments_at_zero_weight(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;

    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    let patch = SegmentPatch {
        ops: vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 30,
            }],
        }],
    };
    segment::patch(&mut conn, &project, segment.clone(), patch)
        .await
        .unwrap();

    // New variant, created after the segment override already exists.
    let beta = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("beta"),
        20,
    )
    .await
    .unwrap();

    // beta is seeded into the segment at 0% - present as an explicit override row, not just
    // defaulting to 0 via COALESCE on a missing row.
    let mut overrides =
        variant::get_segment_weights(&mut conn, segment.id, feature.id, environment.id)
            .await
            .unwrap();
    overrides.sort();
    assert_eq!(overrides, vec![(alt.id, 30), (beta.id, 0)]);

    // The segment's control-variant remainder is untouched by beta's arrival: still 70,
    // exactly as it was before beta existed - no rebalancing was needed.
    let scoped = variant::get_for_feature(&mut conn, &environment, feature.id, Some(segment.id))
        .await
        .unwrap();
    assert_eq!(scoped.iter().find(|v| v.is_control()).unwrap().weight, 70);
    assert_eq!(scoped.iter().find(|v| v.id == beta.id).unwrap().weight, 0);
}

/// Deleting a variant that carries a segment-scoped weight override must remove that
/// override and rebalance the segment's control-variant remainder - otherwise the segment
/// is left overriding a now-nonexistent variant and its remainder is stale.
#[sqlx::test]
async fn deleting_a_variant_rebalances_segment_overrides(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;

    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();
    let beta = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("beta"),
        20,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    let patch = SegmentPatch {
        ops: vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![
                SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                },
                SegmentVariantWeight {
                    variant_id: beta.id,
                    weight: 20,
                },
            ],
        }],
    };
    segment::patch(&mut conn, &project, segment.clone(), patch)
        .await
        .unwrap();

    // Sanity check before deletion: alt=30, beta=20, control remainder=50.
    let scoped = variant::get_for_feature(&mut conn, &environment, feature.id, Some(segment.id))
        .await
        .unwrap();
    assert_eq!(scoped.iter().find(|v| v.id == beta.id).unwrap().weight, 20);
    assert_eq!(scoped.iter().find(|v| v.is_control()).unwrap().weight, 50);

    variant::delete(&mut conn, &environment, &beta)
        .await
        .unwrap();

    let scoped = variant::get_for_feature(&mut conn, &environment, feature.id, Some(segment.id))
        .await
        .unwrap();

    // beta's segment-scoped override row is gone along with the variant itself.
    assert!(!scoped.iter().any(|v| v.id == beta.id));

    // The segment's control-variant remainder is rebalanced to account for beta's weight
    // no longer being part of the segment's total: 100 - alt(30) = 70, not the stale 50.
    assert_eq!(scoped.iter().find(|v| v.is_control()).unwrap().weight, 70);
}

/// Deleting a segment must not leave identities dangling: any `identity_variants` row
/// currently attributed to the deleted segment is dropped by the FK cascade entirely (not
/// just flagged dirty), so the identity reads back as "unassigned" and gets freshly
/// (re-)distributed - here, into the organic pool, since there's no other segment to fall
/// through to - the next time it's read.
#[sqlx::test]
async fn deleting_a_segment_releases_governed_identities(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;

    let before = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(before.segment_id, Some(segment.id));

    segment::delete(&mut conn, &segment).await.unwrap();

    // The identity_variants row was cascade-deleted along with the segment, not merely
    // flagged dirty - it doesn't show up at all until the identity is read again.
    let rows: Vec<IdentityAttribution> =
        SQLIdentities::fetch_identities(&mut *conn, params![environment.id, feature.id])
            .await
            .unwrap();
    assert!(!rows.iter().any(|r| r.identity_id == ident.id));

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, None);
}

/// A pinned assignment survives deletion of a segment whose rules would otherwise match the
/// identity: pins are stored with `segment_id = NULL` (they were never attributed to any
/// segment), so the FK cascade on `identity_variants.segment_id` has nothing to touch here.
#[sqlx::test]
async fn deleting_a_segment_leaves_pinned_identity_untouched(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    identity::override_variant(&mut conn, &environment, &ident, feature.id, alt.id)
        .await
        .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    segment::delete(&mut conn, &segment).await.unwrap();

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.variant_id, alt.id);
    assert!(after.migrated_id.is_none());
    assert_eq!(after.segment_id, None);
}

/// Deleting one segment must not disturb another segment's overrides for the same feature -
/// each segment's `variant_weights` rows are scoped to it alone, so cascade deletion only
/// touches the deleted segment's own rows.
#[sqlx::test]
async fn deleting_a_segment_does_not_affect_other_segments_overrides(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let vip = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        vip.clone(),
        vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 30,
            }],
        }],
    )
    .await;

    let testers = segment::create(&mut conn, &project, "testers".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        testers.clone(),
        vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 55,
            }],
        }],
    )
    .await;

    segment::delete(&mut conn, &vip).await.unwrap();

    // testers' own override is untouched by vip's deletion.
    let scoped = variant::get_for_feature(&mut conn, &environment, feature.id, Some(testers.id))
        .await
        .unwrap();
    assert_eq!(scoped.iter().find(|v| v.id == alt.id).unwrap().weight, 55);
}

/// Staging `SegmentPatchOp::Delete` alongside another op (here, a rename) in the same
/// `SegmentPatch` must delete the segment and ignore the other op entirely - not rename it
/// first and then delete, and not error out.
#[sqlx::test]
async fn patch_delete_ignores_other_staged_ops(mut conn: PoolConnection<Sqlite>) {
    let (project, _environment) = create_context(&mut conn).await;
    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    let patch = SegmentPatch {
        ops: vec![
            SegmentPatchOp::SetName("should-never-be-applied".to_owned()),
            SegmentPatchOp::Delete,
        ],
    };
    let result = segment::patch(&mut conn, &project, segment.clone(), patch)
        .await
        .unwrap();
    assert!(result.is_none());

    assert!(
        segment::get_by_id(&mut conn, &project, segment.id)
            .await
            .is_err()
    );
}

/// `list_overridden_features` (backs "SEGMENT describe") should return one entry per
/// feature the segment overrides, each with its explicit weights plus the control
/// variant's auto-balanced remainder - and nothing for features it doesn't touch.
#[sqlx::test]
async fn list_overridden_features_groups_by_feature(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature_a = create_feature(&mut conn, &environment, "control-a").await;
    let feature_b = create_feature(&mut conn, &environment, "control-b").await;
    let untouched = create_feature(&mut conn, &environment, "control-c").await;

    let alt_a = variant::create(
        &mut conn,
        &environment,
        &feature_a,
        FeatureValue::build("alt-a"),
        40,
    )
    .await
    .unwrap();
    let alt_b = variant::create(
        &mut conn,
        &environment,
        &feature_b,
        FeatureValue::build("alt-b"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    for (feature, alt_id) in [(&feature_a, alt_a.id), (&feature_b, alt_b.id)] {
        let patch = SegmentPatch {
            ops: vec![SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt_id,
                    weight: 25,
                }],
            }],
        };
        segment::patch(&mut conn, &project, segment.clone(), patch)
            .await
            .unwrap();
    }

    let overridden = segment::list_overridden_features(&mut conn, environment.id, segment.id)
        .await
        .unwrap();

    assert_eq!(overridden.len(), 2);
    assert!(!overridden.iter().any(|f| f.feature_id == untouched.id));

    for feature_id in [feature_a.id, feature_b.id] {
        let entry = overridden
            .iter()
            .find(|f| f.feature_id == feature_id)
            .unwrap();
        // One explicit override + one control-variant remainder.
        assert_eq!(entry.weights.len(), 2);
        let explicit = entry.weights.iter().find(|w| !w.is_control).unwrap();
        let control = entry.weights.iter().find(|w| w.is_control).unwrap();
        assert_eq!(explicit.weight, 25);
        assert_eq!(control.weight, 75);
    }
}

/// Distributing with `Some(segment_id)` should converge to that segment's weights and must
/// not perturb the organic (`None`) distribution's accumulators, or vice versa.
#[sqlx::test]
async fn distributor_scopes_by_segment_id(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;

    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let patch = SegmentPatch {
        ops: vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 30,
            }],
        }],
    };
    segment::patch(&mut conn, &project, segment.clone(), patch)
        .await
        .unwrap();

    let mut organic_counts: HashMap<i32, u32> = HashMap::new();
    for _ in 0..100 {
        let v = distributor::distribute(&mut conn, &environment, feature.id, None)
            .await
            .unwrap();
        *organic_counts.entry(v.id).or_default() += 1;
    }
    // Organic weights are 60/40 (control/alt).
    let organic_alt_count = *organic_counts.get(&alt.id).unwrap_or(&0);
    assert!(
        (30..=50).contains(&organic_alt_count),
        "expected ~40 organic picks for alt, got {organic_alt_count}"
    );

    let mut segment_counts: HashMap<i32, u32> = HashMap::new();
    for _ in 0..100 {
        let v = distributor::distribute(&mut conn, &environment, feature.id, Some(segment.id))
            .await
            .unwrap();
        *segment_counts.entry(v.id).or_default() += 1;
    }
    // Segment weights are 70/30 (control/alt).
    let segment_alt_count = *segment_counts.get(&alt.id).unwrap_or(&0);
    assert!(
        (20..=40).contains(&segment_alt_count),
        "expected ~30 segment-scoped picks for alt, got {segment_alt_count}"
    );

    // The two scopes converged to different ratios - confirms they're tracked independently.
    assert_ne!(organic_alt_count, segment_alt_count);
}

/// A segment name failing validation must be rejected without leaving a row behind -
/// validated inside the same transaction as the insert, which rolls back on failure,
/// rather than validated after an already-committed write.
#[sqlx::test]
async fn create_with_invalid_name_is_rejected_and_not_persisted(mut conn: PoolConnection<Sqlite>) {
    let (project, _environment) = create_context(&mut conn).await;

    let result = segment::create(&mut conn, &project, "beta-testers".to_owned(), None).await;
    assert!(result.is_err());

    let segments = segment::get_all(&mut conn, &project, None).await.unwrap();
    assert!(segments.is_empty());
}

// -- reconciliation: already-distributed identities react to segment state changes -------
//
// Reconciliation is now lazy: a segment mutation only flags affected identity_variants rows
// (`segment_dirty = TRUE`), cheaply and synchronously. The actual re-evaluation happens the
// next time each identity is read via `get_identity_variants` (simulated here by `resolve`)
// - mirroring the real per-request flow (`GET .../features`).

/// An identity organically distributed *before* a segment exists gets migrated into that
/// segment's pool once the segment gains a matching rule + a `SetFeatureOverride` - but only
/// once actually read again; the mutation itself just flags the row dirty.
#[sqlx::test]
async fn segment_override_migrates_already_distributed_matching_identity(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    let before = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(before.segment_id, None);

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    // The mutation only flags the row - it hasn't been re-evaluated yet.
    let flagged = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert!(flagged.segment_dirty);
    assert_eq!(flagged.segment_id, None);

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, Some(segment.id));
    assert!(!after.segment_dirty);
}

/// An identity that doesn't match the new segment's rules is left untouched: it does get
/// flagged (marking is a blanket per-feature operation), but re-evaluating confirms no
/// actual change, so it's never redistributed - no accumulator perturbation, no variant flip.
#[sqlx::test]
async fn segment_override_leaves_non_matching_identity_untouched(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let ident =
        identity::get_or_create_by_value(&mut conn, &environment, "someone-else".to_owned())
            .await
            .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    let before = attribution_for(&mut conn, &environment, &feature, ident.id).await;

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.variant_id, before.variant_id);
    assert!(after.migrated_id.is_none());
    assert_eq!(after.segment_id, None);
    assert!(!after.segment_dirty);
}

/// A pinned (explicitly overridden) identity is never flagged by reconciliation at all, even
/// if it matches the new segment's rules.
#[sqlx::test]
async fn segment_override_leaves_pinned_identity_untouched(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    let control = variant::get_for_feature(&mut conn, &environment, feature.id, None)
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.is_control())
        .unwrap();
    identity::override_variant(&mut conn, &environment, &ident, feature.id, control.id)
        .await
        .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    // Not flagged - mark_feature_dirty excludes pinned rows entirely.
    let flagged = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert!(!flagged.segment_dirty);

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.variant_id, control.id);
    assert!(after.migrated_id.is_none());
}

/// Adding a rule to a segment that already has an override (a pure rule edit, no
/// `SetFeatureOverride` in the same batch) retroactively migrates newly-matching identities
/// once they're next read.
#[sqlx::test]
async fn adding_rule_to_already_overriding_segment_retroactively_migrates(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "nobody-yet",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    assert_eq!(
        attribution_for(&mut conn, &environment, &feature, ident.id)
            .await
            .segment_id,
        None
    );

    apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![add_rule(
            "group-1",
            Subject::Identity,
            Comparator::ExactlyMatches,
            "user-vip",
        )],
    )
    .await;

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, Some(segment.id));
}

/// Calling `rule::add` directly (simulating the REST `POST .../rules` endpoint, which
/// bypasses `segment::patch` entirely) still flags reconciliation - not just batched `PATCH`.
#[sqlx::test]
async fn direct_rule_add_triggers_reconciliation_not_just_patch(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    assert_eq!(
        attribution_for(&mut conn, &environment, &feature, ident.id)
            .await
            .segment_id,
        None
    );

    let group_id = segment.groups[0].id;
    rule::add(
        &mut conn,
        segment.id,
        group_id,
        Subject::Identity,
        Comparator::ExactlyMatches,
        "user-vip".to_owned(),
    )
    .await
    .unwrap();

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, Some(segment.id));
}

/// Deleting a rule so a previously-matching, already-migrated identity no longer matches
/// causes it to fall back to the organic pool once next read.
#[sqlx::test]
async fn deleting_a_rule_falls_the_identity_back_to_organic(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 30,
                }],
            },
        ],
    )
    .await;

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-vip".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    assert_eq!(
        attribution_for(&mut conn, &environment, &feature, ident.id)
            .await
            .segment_id,
        Some(segment.id)
    );

    let rule_id = segment.groups[0].rules[0].id;
    rule::delete(&mut conn, segment.id, rule_id).await.unwrap();

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, None);
}

/// Two segments both match an identity; the identity is already correctly attributed to
/// the higher-priority (lower `segment_id`) one. A later, lower-priority segment gaining
/// an override for the same feature must not steal it, once re-evaluated.
#[sqlx::test]
async fn higher_priority_segment_keeps_identity_when_a_later_one_also_matches(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    let older = segment::create(&mut conn, &project, "older".to_owned(), None)
        .await
        .unwrap();
    let older = apply(
        &mut conn,
        &project,
        older,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Environment,
                Comparator::ExactlyMatches,
                &environment.name,
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 15,
                }],
            },
        ],
    )
    .await;

    let ident = identity::get_or_create_by_value(&mut conn, &environment, "any-user".to_owned())
        .await
        .unwrap();
    resolve(&mut conn, &environment, &ident).await;
    assert_eq!(
        attribution_for(&mut conn, &environment, &feature, ident.id)
            .await
            .segment_id,
        Some(older.id)
    );

    let newer = segment::create(&mut conn, &project, "newer".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        newer.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Environment,
                Comparator::ExactlyMatches,
                &environment.name,
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 45,
                }],
            },
        ],
    )
    .await;

    resolve(&mut conn, &environment, &ident).await;

    let after = attribution_for(&mut conn, &environment, &feature, ident.id).await;
    assert_eq!(after.segment_id, Some(older.id));
}

// `in`/`not_in` rule values must be a JSON array, enforced at write time so the
// evaluator (which assumes this at read time) never has to fail closed on bad data

/// `SegmentPatchOp::AddRule` with an `in` comparator and a non-JSON-array value is rejected
/// by `segment::patch`, and the rule is not persisted.
#[sqlx::test]
async fn patch_rejects_invalid_json_for_in_comparator_and_does_not_persist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, _environment) = create_context(&mut conn).await;
    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(&mut conn, &project, segment, vec![add_group(None)]).await;

    let result = segment::patch(
        &mut conn,
        &project,
        segment.clone(),
        SegmentPatch {
            ops: vec![add_rule(
                "group-1",
                Subject::Trait("plan".to_owned()),
                Comparator::In,
                "not-json",
            )],
        },
    )
    .await;
    assert!(result.is_err());

    let reloaded = segment::get_by_id(&mut conn, &project, segment.id)
        .await
        .unwrap();
    assert!(reloaded.groups[0].rules.is_empty());
}

/// Calling `rule::add` directly (simulating the REST `POST .../rules` endpoint, which
/// bypasses `segment::patch` entirely) rejects the same invalid value for `not_in`, and
/// does not persist it either.
#[sqlx::test]
async fn direct_rule_add_rejects_invalid_json_for_not_in_comparator_and_does_not_persist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, _environment) = create_context(&mut conn).await;
    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(&mut conn, &project, segment, vec![add_group(None)]).await;
    let group_id = segment.groups[0].id;

    let result = rule::add(
        &mut conn,
        segment.id,
        group_id,
        Subject::Trait("plan".to_owned()),
        Comparator::NotIn,
        "not-json".to_owned(),
    )
    .await;
    assert!(result.is_err());

    let reloaded = segment::get_by_id(&mut conn, &project, segment.id)
        .await
        .unwrap();
    assert!(reloaded.groups[0].rules.is_empty());
}

/// A well-formed JSON array value for `in` is accepted and persisted normally - guards
/// against the validation above being overly strict.
#[sqlx::test]
async fn valid_json_array_for_in_comparator_is_persisted(mut conn: PoolConnection<Sqlite>) {
    let (project, _environment) = create_context(&mut conn).await;
    let segment = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(None),
            add_rule(
                "group-1",
                Subject::Trait("plan".to_owned()),
                Comparator::In,
                r#"["pro","enterprise"]"#,
            ),
        ],
    )
    .await;

    assert_eq!(segment.groups[0].rules.len(), 1);
    assert_eq!(segment.groups[0].rules[0].value, r#"["pro","enterprise"]"#);
}
