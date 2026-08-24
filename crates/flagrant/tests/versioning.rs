use flagrant::errors::FlagrantError;
use flagrant::models::{commit, segment};
use flagrant_types::payload::{
    CommitPayload, FeatureCommitPart, FeaturePatch, SegmentCommitPart, SegmentPatch,
    SegmentPatchOp,
};
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{create_context, create_feature, random_string};

mod common;

#[sqlx::test]
async fn stale_feature_version_is_rejected(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;
    assert_eq!(feature.version, 1);

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: Some(feature.version),
            patch: FeaturePatch {
                description: Some("first edit".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    // Retry with the now-stale version.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: Some(feature.version),
            patch: FeaturePatch {
                description: Some("second edit".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let err = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap_err();

    assert!(err.downcast_ref::<FlagrantError>().is_some_and(|e| matches!(
        e,
        FlagrantError::VersionMismatch {
            kind: "feature",
            expected: 1,
            current: 2,
            ..
        }
    )));
}

#[sqlx::test]
async fn matching_feature_version_succeeds_and_bumps_by_one(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: Some(feature.version),
            patch: FeaturePatch {
                description: Some("edited".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    assert_eq!(result.feature.unwrap().version, feature.version + 1);
}

#[sqlx::test]
async fn matching_segment_version_succeeds_and_bumps_by_one(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            version: Some(seg.version),
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::SetDescription(Some(
                    "new description".to_owned(),
                ))],
            },
        }),
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    assert_eq!(result.segment.unwrap().version, seg.version + 1);
}

#[sqlx::test]
async fn omitted_version_skips_check(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    // Bump the feature's version once, "behind the client's back".
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: None,
            patch: FeaturePatch {
                description: Some("first edit".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    // A second commit with no version at all should still succeed, even though it
    // would be stale if checked.
    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: None,
            patch: FeaturePatch {
                description: Some("second edit".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let result = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    assert_eq!(result.feature.unwrap().description, "second edit");
}

#[sqlx::test]
async fn modifying_already_deleted_feature_is_rejected(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "hello").await;

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: Some(feature.version),
            patch: FeaturePatch {
                delete: true,
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    let payload = CommitPayload {
        comment: None,
        feature: Some(FeatureCommitPart {
            id: feature.id,
            version: Some(feature.version),
            patch: FeaturePatch {
                description: Some("resurrection attempt".to_owned()),
                ..Default::default()
            },
        }),
        identity: None,
        segment: None,
    };
    let err = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap_err();

    // Unlike segments, `feature::get_by_id` maps a missing row to `QueryFailed` rather
    // than `NotFound` - still rejected, just via a different existing error variant.
    assert!(
        err.downcast_ref::<FlagrantError>()
            .is_some_and(|e| matches!(e, FlagrantError::QueryFailed(..)))
    );
}

#[sqlx::test]
async fn modifying_already_deleted_segment_still_not_found(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            version: Some(seg.version),
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::Delete],
            },
        }),
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            version: Some(seg.version),
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::SetDescription(Some(
                    "resurrection attempt".to_owned(),
                ))],
            },
        }),
    };
    let err = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap_err();

    assert!(
        err.downcast_ref::<FlagrantError>()
            .is_some_and(|e| matches!(e, FlagrantError::NotFound(_)))
    );
}

/// Groups/rules have no version column of their own - they're only ever addressed
/// through their owning segment's `SegmentPatchOp`, so a rule-add op is gated
/// transitively through the segment's own version.
#[sqlx::test]
async fn rule_op_is_gated_by_owning_segment_version(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let seg = segment::create(
        &mut conn,
        &project,
        format!("SEG_{}", random_string(8)),
        None,
    )
    .await
    .unwrap();

    // Bump the segment's version via an AddGroup op, "behind the client's back".
    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            version: Some(seg.version),
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::AddGroup {
                    connector: None,
                    description: None,
                }],
            },
        }),
    };
    commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap();

    // Attempt an AddRule op against the now-stale version.
    let payload = CommitPayload {
        comment: None,
        feature: None,
        identity: None,
        segment: Some(SegmentCommitPart {
            id: seg.id,
            version: Some(seg.version),
            patch: SegmentPatch {
                ops: vec![SegmentPatchOp::AddRule {
                    group_label: "group-1".to_owned(),
                    subject: flagrant_types::Subject::Identity,
                    comparator: flagrant_types::Comparator::ExactlyMatches,
                    value: "someone".to_owned(),
                }],
            },
        }),
    };
    let err = commit::apply(&mut conn, &project, &environment, payload)
        .await
        .unwrap_err();

    assert!(err.downcast_ref::<FlagrantError>().is_some_and(|e| matches!(
        e,
        FlagrantError::VersionMismatch {
            kind: "segment",
            ..
        }
    )));
}
