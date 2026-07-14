use flagrant::{
    evaluator,
    models::{identity, segment, variant},
};
use flagrant_types::{
    Comparator, FeatureValue, GroupConnector, Project, Segment, SegmentDriver,
    payload::{SegmentPatch, SegmentPatchOp, SegmentVariantWeight},
};
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{create_context, create_feature};

mod common;

/// Stages and commits `ops` against `segment` in one patch call.
async fn apply(
    conn: &mut PoolConnection<Sqlite>,
    project: &Project,
    segment: Segment,
    ops: Vec<SegmentPatchOp>,
) -> Segment {
    segment::patch(conn, project, segment, SegmentPatch { ops })
        .await
        .unwrap()
}

fn add_rule(
    group_label: &str,
    driver: SegmentDriver,
    comparator: Comparator,
    value: &str,
) -> SegmentPatchOp {
    SegmentPatchOp::AddRule {
        group_label: group_label.to_owned(),
        driver,
        comparator,
        value: value.to_owned(),
    }
}

fn add_group(connector: Option<GroupConnector>) -> SegmentPatchOp {
    SegmentPatchOp::AddGroup {
        connector,
        description: None,
    }
}

#[sqlx::test]
async fn no_segment_overriding_feature_returns_none(mut conn: PoolConnection<Sqlite>) {
    let (_project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "control").await;

    let identity = identity::create(&mut conn, &environment, "user-1".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_none());
}

#[sqlx::test]
async fn matching_segment_returns_its_full_variant_weight_breakdown(
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

    let control_id = feature.get_default_variant().id;
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
                SegmentDriver::Identity,
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

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let mut result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap()
        .expect("segment should match");

    let mut expected = vec![(alt.id, 30), (control_id, 70)];

    result.sort();
    expected.sort();

    assert_eq!(result, expected);
}

#[sqlx::test]
async fn segment_with_non_matching_rule_returns_none(mut conn: PoolConnection<Sqlite>) {
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

    apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                SegmentDriver::Identity,
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

    let identity = identity::create(
        &mut conn,
        &environment,
        "some-other-user".to_owned(),
        vec![],
    )
    .await
    .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_none());
}

/// Two segments override the same feature; only the second (higher segment_id) matches.
/// The evaluator must not stop at the first (non-matching) candidate.
#[sqlx::test]
async fn falls_through_a_non_matching_segment_to_a_later_matching_one(
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

    let first = segment::create(&mut conn, &project, "earlybird".to_owned(), None)
        .await
        .unwrap();

    apply(
        &mut conn,
        &project,
        first.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "someone-else",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 10,
                }],
            },
        ],
    )
    .await;

    let second = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();

    apply(
        &mut conn,
        &project,
        second.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                SegmentDriver::Identity,
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

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap()
        .expect("second segment should match");

    assert!(
        result
            .iter()
            .any(|(id, weight)| *id == alt.id && *weight == 30)
    );
}

/// Two segments override the same feature and both match the identity - the first-created
/// (lower segment_id) wins.
#[sqlx::test]
async fn first_created_segment_wins_when_multiple_match(mut conn: PoolConnection<Sqlite>) {
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

    // Both segments match every identity in "prod" via the Environment driver.
    let older = segment::create(&mut conn, &project, "older".to_owned(), None)
        .await
        .unwrap();

    apply(
        &mut conn,
        &project,
        older.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                SegmentDriver::Environment,
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
                SegmentDriver::Environment,
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

    assert!(older.id < newer.id);

    let identity = identity::create(&mut conn, &environment, "any-user".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap()
        .expect("a segment should match");

    // The older segment's weight (15), not the newer one's (45).
    assert!(
        result
            .iter()
            .any(|(id, weight)| *id == alt.id && *weight == 15)
    );
}

/// A group's rules are OR-ed: only one of two rules needs to match for the group (and, with
/// a single-group segment, the whole segment) to match.
#[sqlx::test]
async fn rules_within_a_group_are_or_ed(mut conn: PoolConnection<Sqlite>) {
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

    let segment = segment::create(&mut conn, &project, "either".to_owned(), None)
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
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "nobody",
            ),
            add_rule(
                "group-1",
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 20,
                }],
            },
        ],
    )
    .await;

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_some());
}

/// Multi-group segment: `[group-1] AND NOT [group-2]`. The identity matches group-1 (VIP)
/// but not group-2 (banned), so the segment should match.
#[sqlx::test]
async fn groups_combine_via_and_and_and_not(mut conn: PoolConnection<Sqlite>) {
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

    let segment = segment::create(&mut conn, &project, "vip_not_banned".to_owned(), None)
        .await
        .unwrap();
    let segment = apply(
        &mut conn,
        &project,
        segment.clone(),
        vec![
            add_group(None),
            add_rule(
                "group-1",
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
        ],
    )
    .await;

    apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(Some(GroupConnector::AndNot)),
            add_rule(
                "group-2",
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "banned-user",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 20,
                }],
            },
        ],
    )
    .await;

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_some());
}

/// A non-head group with zero rules never contributes a match, so a segment relying on it
/// alone (via AND) never matches.
#[sqlx::test]
async fn empty_non_head_group_blocks_the_segment_from_matching(mut conn: PoolConnection<Sqlite>) {
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

    let segment = segment::create(&mut conn, &project, "broken".to_owned(), None)
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
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-vip",
            ),
        ],
    )
    .await;

    apply(
        &mut conn,
        &project,
        segment,
        vec![
            add_group(Some(GroupConnector::And)),
            // group-2 has no rules - AND with an always-false group.
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 20,
                }],
            },
        ],
    )
    .await;

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_none());
}

/// A segment with zero groups (created but never given targeting criteria) never matches,
/// even though it has a feature override configured.
#[sqlx::test]
async fn segment_with_zero_groups_never_matches(mut conn: PoolConnection<Sqlite>) {
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

    let segment = segment::create(&mut conn, &project, "groupless".to_owned(), None)
        .await
        .unwrap();

    apply(
        &mut conn,
        &project,
        segment,
        vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: feature.id,
            environment_id: environment.id,
            variant_weights: vec![SegmentVariantWeight {
                variant_id: alt.id,
                weight: 20,
            }],
        }],
    )
    .await;

    let identity = identity::create(&mut conn, &environment, "user-vip".to_owned(), vec![])
        .await
        .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_none());
}

/// A `Trait` driver rule matches against a trait loaded from the DB - exercising the real
/// `IdentityWithTraits` shape (not the synthetic one used by evaluator.rs's unit tests).
#[sqlx::test]
async fn trait_driver_matches_a_db_loaded_trait(mut conn: PoolConnection<Sqlite>) {
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

    let segment = segment::create(&mut conn, &project, "premium".to_owned(), None)
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
                SegmentDriver::Trait("plan".to_owned()),
                Comparator::ExactlyMatches,
                "premium",
            ),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: feature.id,
                environment_id: environment.id,
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 20,
                }],
            },
        ],
    )
    .await;

    let identity = identity::create(
        &mut conn,
        &environment,
        "user-1".to_owned(),
        vec![flagrant_types::payload::IdentityTraitPayload {
            name: "plan".to_owned(),
            value: Some(flagrant_types::TraitValue::Str("premium".to_owned())),
        }],
    )
    .await
    .unwrap();

    let result = evaluator::evaluate(&mut conn, &environment, &identity, feature.id)
        .await
        .unwrap();

    assert!(result.is_some());
}
