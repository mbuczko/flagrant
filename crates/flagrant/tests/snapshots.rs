use flagrant::models::{commit, identity, segment, snapshot};
use flagrant_types::{
    Comparator, FeatureValue, Subject,
    payload::{
        CommitPayload, FeatureCommitPart, FeaturePatch, IdentityCommitPart, IdentityOverridePatch,
        IdentityPatch, SegmentCommitPart, SegmentPatch, SegmentPatchOp, SegmentVariantWeight,
        VariantPatchOp,
    },
};
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{add_group, add_rule, apply, create_context, create_feature, random_string};

mod common;

/// Every commit that touches a feature's own patch should record exactly one new
/// snapshot, with strictly increasing versions per (feature, environment) - never
/// reused, even across many commits.
#[sqlx::test]
async fn commit_creates_snapshot_with_incrementing_version(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let payload = CommitPayload {
        comment: Some("first".to_owned()),
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                description: Some("v1 description".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    assert_eq!(result.snapshots.len(), 1);
    assert_eq!(result.snapshots[0].version, 1);
    assert_eq!(result.snapshots[0].comment.as_deref(), Some("first"));

    let payload = CommitPayload {
        comment: Some("second".to_owned()),
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                description: Some("v2 description".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    assert_eq!(result.snapshots.len(), 1);
    assert_eq!(result.snapshots[0].version, 2);

    let all = snapshot::list(&mut conn, feature.id, environment.id)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}

/// Restoring to an earlier version should reproduce that version's variants/weights,
/// and restoring is itself a commit - it produces one brand-new snapshot rather than
/// rewriting history in place.
#[sqlx::test]
async fn restore_reproduces_variants_and_weights(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    // v1: add a non-control variant at 40%.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![VariantPatchOp::Add {
                    value: FeatureValue::Text("variant-a".to_owned()),
                    weight: 40,
                }],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    let v1 = result.snapshots[0].version;
    let feature_v1 = result.feature.unwrap();
    let variant_a_id = feature_v1
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    // v2: bump the variant's weight to 70%.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![VariantPatchOp::SetWeight {
                    id: variant_a_id,
                    weight: 70,
                }],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    // Restore to v1: the variant should be back at 40%.
    let feature = flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let restored_snapshot =
        snapshot::restore(&mut conn, &project, &environment, &feature, v1, None)
            .await
            .unwrap();

    let restored_state = restored_snapshot.parsed_state().unwrap();
    let restored_variant = restored_state
        .variants
        .iter()
        .find(|v| !v.is_control)
        .unwrap();
    assert_eq!(restored_variant.weight, 40);
    assert_eq!(
        restored_variant.value,
        FeatureValue::Text("variant-a".to_owned())
    );

    // Restoring must itself be a new, third snapshot - never renumbering v1/v2.
    assert_eq!(restored_snapshot.version, 3);
    assert!(
        restored_snapshot
            .comment
            .as_deref()
            .unwrap()
            .contains(&format!("v{v1}"))
    );
}

/// If the variant a snapshot references was deleted (and, per the reconciliation rule,
/// no live variant shares its value either), restore must recreate it under a fresh id
/// rather than failing.
#[sqlx::test]
async fn restore_recreates_deleted_variant_under_new_id(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![VariantPatchOp::Add {
                    value: FeatureValue::Text("variant-a".to_owned()),
                    weight: 50,
                }],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    let v1 = result.snapshots[0].version;
    let feature_v1 = result.feature.unwrap();
    let old_variant_id = feature_v1
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    // v2: delete that variant entirely.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![VariantPatchOp::Delete { id: old_variant_id }],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    let feature = flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.variants.len(), 1, "only the control variant left");

    let restored_snapshot =
        snapshot::restore(&mut conn, &project, &environment, &feature, v1, None)
            .await
            .unwrap();
    let restored_feature =
        flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
            .await
            .unwrap();

    let recreated = restored_feature
        .variants
        .iter()
        .find(|v| !v.is_control())
        .expect("variant-a should have been recreated");
    assert_ne!(
        recreated.id, old_variant_id,
        "recreated variant must get a fresh id, never reusing the deleted one"
    );
    assert_eq!(recreated.value, FeatureValue::Text("variant-a".to_owned()));
    assert_eq!(recreated.weight, 50);
    assert!(restored_snapshot.version > v1);
}

/// A snapshot's pinned identity overrides should be restored by remapping through the
/// same variant reconciliation used for the variants themselves - including when the
/// pinned variant was deleted and recreated in between.
#[sqlx::test]
async fn restore_restores_pinned_identity_override(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    let ident = identity::get_or_create_by_value(&mut conn, &environment, "user-1".to_owned())
        .await
        .unwrap();

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![VariantPatchOp::Add {
                    value: FeatureValue::Text("variant-a".to_owned()),
                    weight: 50,
                }],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    let feature_v1 = result.feature.unwrap();
    let variant_a_id = feature_v1
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    // Pin the identity to variant-a via an identity commit part.
    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: Some(IdentityCommitPart {
            value: ident.value.clone(),
            patch: IdentityPatch {
                overrides: vec![IdentityOverridePatch {
                    feature_name: feature.name.clone(),
                    variant_value: "variant-a".to_owned(),
                }],
                ..Default::default()
            },
        }),
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();
    // The pin cascaded into a snapshot for the feature even though no feature part was sent.
    assert_eq!(result.snapshots.len(), 1);
    assert_eq!(result.snapshots[0].feature_id, feature.id);
    let v_pinned = result.snapshots[0].version;

    let assigned = identity::get_variant_for_identity(&mut conn, &environment, feature.id, &ident)
        .await
        .unwrap();
    assert_eq!(assigned, Some(variant_a_id));

    // Delete and re-add the variant so its id changes, then restore to the pinned snapshot.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                variants: vec![
                    VariantPatchOp::Delete { id: variant_a_id },
                    VariantPatchOp::Add {
                        value: FeatureValue::Text("variant-b".to_owned()),
                        weight: 20,
                    },
                ],
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    // The pin is gone now - deleting the variant detaches it.
    let assigned = identity::get_variant_for_identity(&mut conn, &environment, feature.id, &ident)
        .await
        .unwrap();
    assert_eq!(assigned, None);

    let feature = flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    snapshot::restore(&mut conn, &project, &environment, &feature, v_pinned, None)
        .await
        .unwrap();

    let restored_feature =
        flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
            .await
            .unwrap();
    let recreated_variant_a = restored_feature
        .variants
        .iter()
        .find(|v| v.value == FeatureValue::Text("variant-a".to_owned()))
        .expect("variant-a should have been recreated by restore");

    let assigned = identity::get_variant_for_identity(&mut conn, &environment, feature.id, &ident)
        .await
        .unwrap();
    assert_eq!(
        assigned,
        Some(recreated_variant_a.id),
        "the pin should be restored against the recreated variant's new id"
    );
}

/// A segment-only commit (no feature in context) that changes a feature's weight override
/// must still produce a snapshot for that feature - otherwise the feature's history would
/// silently miss a real change to its effective behavior.
#[sqlx::test]
async fn segment_weight_only_commit_cascades_snapshot_to_overridden_feature(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    let feature = flagrant::models::feature::patch(
        &mut conn,
        &environment,
        &feature,
        FeaturePatch {
            variants: vec![VariantPatchOp::Add {
                value: FeatureValue::Text("variant-a".to_owned()),
                weight: 50,
            }],
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let variant_a_id = feature
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    let payload = CommitPayload {
        comment: Some("weight override".to_owned()),
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::SetFeatureOverride {
                    feature_id: feature.id,
                    environment_id: environment.id,
                    variant_weights: vec![SegmentVariantWeight {
                        variant_id: variant_a_id,
                        weight: 80,
                    }],
                }],
            },
        }),
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    assert_eq!(result.snapshots.len(), 1);
    assert_eq!(result.snapshots[0].feature_id, feature.id);
    assert_eq!(result.snapshots[0].environment_id, environment.id);

    let state = result.snapshots[0].parsed_state().unwrap();
    assert_eq!(state.segment_overrides.len(), 1);
    assert_eq!(state.segment_overrides[0].segment_id, seg.id);
    assert!(
        state.segment_overrides[0]
            .weights
            .iter()
            .any(|w| w.variant_id == variant_a_id && w.weight == 80)
    );
}

/// A single commit that stages both a feature patch and a segment override touching that
/// same feature must produce exactly one new snapshot for it, not two - "every commit
/// creates a snapshot" means every distinct patch application coordinated within one
/// atomic operation, not one write per underlying resource touched.
#[sqlx::test]
async fn combined_feature_and_segment_commit_produces_one_snapshot(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    let feature = flagrant::models::feature::patch(
        &mut conn,
        &environment,
        &feature,
        FeaturePatch {
            variants: vec![VariantPatchOp::Add {
                value: FeatureValue::Text("variant-a".to_owned()),
                weight: 50,
            }],
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let variant_a_id = feature
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    let payload = CommitPayload {
        comment: Some("combined".to_owned()),
        feature: Some(FeatureCommitPart {
            id: feature.id,
            patch: FeaturePatch {
                description: Some("updated".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::SetFeatureOverride {
                    feature_id: feature.id,
                    environment_id: environment.id,
                    variant_weights: vec![SegmentVariantWeight {
                        variant_id: variant_a_id,
                        weight: 60,
                    }],
                }],
            },
        }),
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    assert_eq!(
        result.snapshots.len(),
        1,
        "one feature touched by both parts of one commit should get exactly one snapshot"
    );
    let state = result.snapshots[0].parsed_state().unwrap();
    assert_eq!(state.description, "updated");
    assert_eq!(state.segment_overrides.len(), 1);
}

/// If the segment overriding a feature is later deleted, restoring an older snapshot that
/// referenced it must recreate the segment (with equivalent rules/weights) rather than
/// silently dropping the override - a bare `segment_id` reference wouldn't survive that.
#[sqlx::test]
async fn restore_recreates_deleted_overriding_segment(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    let feature = flagrant::models::feature::patch(
        &mut conn,
        &environment,
        &feature,
        FeaturePatch {
            variants: vec![VariantPatchOp::Add {
                value: FeatureValue::Text("variant-a".to_owned()),
                weight: 50,
            }],
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let variant_a_id = feature
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    let segment_name = format!("SEG_{}", random_string(8));
    let seg = segment::create(&mut conn, &project, segment_name.clone(), None)
        .await
        .unwrap();
    let seg = apply(&mut conn, &project, seg, vec![add_group(None)]).await;
    let seg = apply(
        &mut conn,
        &project,
        seg,
        vec![add_rule(
            "group-1",
            Subject::Identity,
            Comparator::ExactlyMatches,
            "user-1",
        )],
    )
    .await;
    let seg = apply(
        &mut conn,
        &project,
        seg,
        vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: variant_a_id,
                weight: 90,
            }],
        }],
    )
    .await;

    // Snapshot the feature now, with the segment override attached.
    let snap = snapshot::capture(
        &mut conn,
        &project,
        &environment,
        &feature,
        Some("with segment override".to_owned()),
    )
    .await
    .unwrap();

    // Delete the segment entirely.
    segment::delete(&mut conn, &seg).await.unwrap();
    assert!(
        segment::get_by_id(&mut conn, &project, seg.id)
            .await
            .is_err()
    );

    let feature = flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    snapshot::restore(
        &mut conn,
        &project,
        &environment,
        &feature,
        snap.version,
        None,
    )
    .await
    .unwrap();

    let overrides = segment::list_overrides_for_feature(&mut conn, environment.id, feature.id)
        .await
        .unwrap();
    assert_eq!(overrides.len(), 1);
    let (new_segment_id, new_segment_name, weights) = &overrides[0];
    assert_ne!(
        *new_segment_id, seg.id,
        "recreated segment must get a fresh id"
    );
    assert_eq!(new_segment_name, &segment_name);
    assert!(weights.iter().any(|w| w.weight == 90));

    let recreated = segment::get_by_id(&mut conn, &project, *new_segment_id)
        .await
        .unwrap();
    assert_eq!(recreated.groups.len(), 1);
    assert_eq!(recreated.groups[0].rules.len(), 1);
}

/// Restoring to a snapshot taken *before* a segment override existed must remove that
/// override, not just leave it untouched - the target snapshot's `segment_overrides` is
/// empty, so a segment currently overriding the feature that isn't part of that list is
/// stale relative to the version being restored to and must be cleared, exactly like a
/// deleted variant or an identity pin added afterward already are.
#[sqlx::test]
async fn restore_clears_segment_override_added_after_the_target_snapshot(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    let feature = flagrant::models::feature::patch(
        &mut conn,
        &environment,
        &feature,
        FeaturePatch {
            variants: vec![VariantPatchOp::Add {
                value: FeatureValue::Text("variant-a".to_owned()),
                weight: 50,
            }],
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let variant_a_id = feature
        .variants
        .iter()
        .find(|v| !v.is_control())
        .unwrap()
        .id;

    // v1: snapshot taken with no segment override in place yet.
    let v1 = snapshot::capture(&mut conn, &project, &environment, &feature, None)
        .await
        .unwrap()
        .version;

    // A segment is created and later overrides the feature - no snapshot is taken for the
    // segment's own creation (it doesn't override anything yet), matching "COMMIT on a
    // segment with no feature touched creates no snapshot."
    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    let payload = CommitPayload {
        comment: Some("add override".to_owned()),
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::SetFeatureOverride {
                    feature_id: feature.id,
                    environment_id: environment.id,
                    variant_weights: vec![SegmentVariantWeight {
                        variant_id: variant_a_id,
                        weight: 70,
                    }],
                }],
            },
        }),
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    let overrides = segment::list_overrides_for_feature(&mut conn, environment.id, feature.id)
        .await
        .unwrap();
    assert_eq!(overrides.len(), 1, "override should be live before restore");

    // Restore to v1 - a version that predates the segment override entirely.
    let feature = flagrant::models::feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    snapshot::restore(&mut conn, &project, &environment, &feature, v1, None)
        .await
        .unwrap();

    let overrides = segment::list_overrides_for_feature(&mut conn, environment.id, feature.id)
        .await
        .unwrap();
    assert!(
        overrides.is_empty(),
        "restoring to a pre-override snapshot must clear the override, not leave it in place"
    );
}

/// A snapshot's comment can be changed after the fact, without touching its state or
/// version - "SNAPSHOT describe" is a comment-only edit, not a restore.
#[sqlx::test]
async fn set_comment_updates_only_the_comment(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let captured = snapshot::capture(
        &mut conn,
        &project,
        &environment,
        &feature,
        Some("original".to_owned()),
    )
    .await
    .unwrap();

    let updated = snapshot::set_comment(
        &mut conn,
        feature.id,
        environment.id,
        captured.version,
        Some("revised".to_owned()),
    )
    .await
    .unwrap();

    assert_eq!(updated.version, captured.version);
    assert_eq!(updated.comment.as_deref(), Some("revised"));
    assert_eq!(updated.state, captured.state);

    let cleared = snapshot::set_comment(&mut conn, feature.id, environment.id, captured.version, None)
        .await
        .unwrap();
    assert_eq!(cleared.comment, None);
}

/// Updating the comment of a non-existent snapshot version should fail loudly, not
/// silently no-op.
#[sqlx::test]
async fn set_comment_on_missing_snapshot_errors(mut conn: PoolConnection<Sqlite>) {
    let (_project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let result = snapshot::set_comment(&mut conn, feature.id, environment.id, 99, None).await;
    assert!(result.is_err());
}
