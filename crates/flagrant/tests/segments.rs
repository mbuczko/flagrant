use std::collections::HashMap;

use flagrant::{
    distributor,
    models::{segment, variant},
};
use flagrant_types::{
    FeatureValue,
    payload::{SegmentPatch, SegmentPatchOp, SegmentVariantWeight},
};
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{create_context, create_feature};

mod common;

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
    let displayed = segment::list_overrides_for_feature(&mut conn, feature.id, environment.id)
        .await
        .unwrap();
    let (_, weights) = displayed.iter().find(|(name, _)| name == "vip").unwrap();
    let mut by_id: Vec<(i32, u8)> = weights.iter().map(|w| (w.variant_id, w.weight)).collect();
    let mut expected = vec![(alt.id, 30), (control.id, 70)];

    by_id.sort();
    expected.sort();

    assert_eq!(by_id, expected);
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

    // The two scopes converged to different ratios — confirms they're tracked independently.
    assert_ne!(organic_alt_count, segment_alt_count);
}
