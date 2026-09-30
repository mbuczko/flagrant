//! An identity that leaves a distribution pool (organic, or a segment's) has to hand its
//! accumulator draw back, or the population drifts away from the configured weights.
//!
//! A feature is defined as dark 50% (control) / light 30% / auto 20% and 100 identities are
//! distributed across it - exactly 50/30/20, since the accumulator hands out precisely
//! proportional draws over any 100 consecutive hits. Then a few of them take a detour - into
//! a segment overriding the weights (dark 70 / light 10 / auto 20), into a pin, out of the
//! distribution, or out of existence - and come back (or are replaced). The population must
//! end up 50/30/20 again, whichever identities happened to take the detour.
//!
//! Before `distributor::release` existed, each identity coming back drew afresh from the
//! organic accumulator without the slot it previously held being given back, so the 4
//! draws (always the head of a new cycle: 2 dark, 1 light, 1 auto) landed on top of
//! whatever the 4 had been carrying - e.g. 52 / 29 / 19 for identities that held 0 dark,
//! 2 light and 2 auto, 48 / 31 / 21 for 4 dark ones.
//!
//! Identities that change pool also keep their current variant when the new pool has room
//! for it (`distributor::redistribute`), so a weight change moves about as many identities
//! as it has to instead of reshuffling nearly all of them with a fresh draw each.

use std::collections::HashMap;

use flagrant::{
    distributor,
    models::{
        feature,
        identity::{self, HugSql, SQLIdentities},
        segment, variant,
    },
};
use flagrant_types::{
    Comparator, Environment, Feature, Project, Segment, Subject, Variant, VariantValue,
    payload::{SegmentPatchOp, SegmentVariantWeight},
};
use hugsqlx::params;
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{add_group, add_rule, apply, create_context};

mod common;

const IDENTITIES: usize = 100;

type Population = HashMap<String, &'static str>;
type Tally = (i32, i32, i32);

/// What the 4 identities that take the detour originally held, as (variant, how many) picks.
/// Each of these yields a different drift without the fix.
const HELD_LIGHT_AND_AUTO: &[(&str, usize)] = &[("light", 2), ("auto", 2)];
const HELD_DARK: &[(&str, usize)] = &[("dark", 4)];
const HELD_BALANCED: &[(&str, usize)] = &[("dark", 2), ("light", 1), ("auto", 1)];

#[derive(Debug, sqlx::FromRow)]
struct Assignment {
    identity: String,
    variant_id: i32,
    migrated_id: Option<i32>,
}

struct Fixture {
    project: Project,
    environment: Environment,
    feature: Feature,
    dark: Variant,
    light: Variant,
    auto: Variant,
}

fn names() -> Vec<String> {
    (1..=IDENTITIES).map(|n| format!("user-{n}")).collect()
}

/// (dark, light, auto) head-counts of a population.
fn tally(population: &Population) -> Tally {
    let count = |name| population.values().filter(|v| **v == name).count() as i32;
    (count("dark"), count("light"), count("auto"))
}

/// Reads the feature on behalf of the given identities, which distributes the new ones and
/// re-evaluates the ones flagged `segment_dirty`.
async fn read(conn: &mut PoolConnection<Sqlite>, environment: &Environment, names: &[String]) {
    for name in names {
        let ident = identity::get_or_create_by_value(conn, environment, name.clone())
            .await
            .unwrap();
        identity::get_identity_variants(conn, environment, &ident)
            .await
            .unwrap();
    }
}

async fn population(conn: &mut PoolConnection<Sqlite>, f: &Fixture) -> Population {
    let rows: Vec<Assignment> =
        SQLIdentities::fetch_identities(&mut **conn, params![f.environment.id, f.feature.id])
            .await
            .unwrap();

    rows.into_iter()
        .map(|row| {
            // effective variant: a pending migration wins over the one originally attached
            let variant_id = row.migrated_id.unwrap_or(row.variant_id);
            let name = if variant_id == f.light.id {
                "light"
            } else if variant_id == f.auto.id {
                "auto"
            } else {
                "dark"
            };
            (row.identity, name)
        })
        .collect()
}

/// Defines the feature and distributes 100 identities across it.
async fn setup(conn: &mut PoolConnection<Sqlite>) -> Fixture {
    let (project, environment) = create_context(conn).await;
    let feature = feature::create(
        conn,
        &environment,
        "theme".to_owned(),
        None,
        VariantValue::build("dark"),
        true,
        false,
    )
    .await
    .unwrap();
    let light = variant::create(
        conn,
        &environment,
        &feature,
        VariantValue::build("light"),
        30,
    )
    .await
    .unwrap();
    let auto = variant::create(
        conn,
        &environment,
        &feature,
        VariantValue::build("auto"),
        20,
    )
    .await
    .unwrap();
    let dark = feature
        .variants
        .iter()
        .find(|v| v.value == VariantValue::build("dark"))
        .cloned()
        .unwrap();

    let fixture = Fixture {
        project,
        environment,
        feature,
        dark,
        light,
        auto,
    };
    read(conn, &fixture.environment, &names()).await;
    assert_eq!(
        tally(&population(conn, &fixture).await),
        (50, 30, 20),
        "organic distribution is exact"
    );
    fixture
}

/// Picks 4 identities out of the initially distributed ones - (variant, how many) each.
async fn pick(
    conn: &mut PoolConnection<Sqlite>,
    f: &Fixture,
    picks: &[(&str, usize)],
) -> Vec<String> {
    let population = population(conn, f).await;
    let mut ordered: Vec<_> = population.iter().collect();
    ordered.sort();

    let picked: Vec<String> = picks
        .iter()
        .flat_map(|(name, n)| {
            ordered
                .iter()
                .filter(move |(_, v)| **v == *name)
                .take(*n)
                .map(|(i, _)| (*i).clone())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(picked.len(), 4);
    picked
}

/// A segment matching exactly the given identities and overriding the feature's weights.
async fn override_segment(
    conn: &mut PoolConnection<Sqlite>,
    f: &Fixture,
    matching: &[String],
) -> Segment {
    let segment = segment::create(conn, &f.project, "picked".to_owned(), None)
        .await
        .unwrap();
    let mut ops = vec![add_group(None)];
    ops.extend(
        matching
            .iter()
            .map(|i| add_rule("group-1", Subject::Identity, Comparator::ExactlyMatches, i)),
    );
    // dark is the control variant, so it takes the remainder: 100 - 10 - 20 = 70
    ops.push(SegmentPatchOp::SetFeatureOverride {
        feature_id: f.feature.id,
        variant_weights: vec![
            SegmentVariantWeight {
                variant_id: f.light.id,
                weight: 10,
            },
            SegmentVariantWeight {
                variant_id: f.auto.id,
                weight: 20,
            },
        ],
    });
    apply(conn, &f.project, &f.environment, segment, ops).await
}

async fn segment_accumulators(
    conn: &mut PoolConnection<Sqlite>,
    f: &Fixture,
    segment: &Segment,
) -> Vec<(i32, i32)> {
    let mut acc: Vec<_> =
        variant::get_for_feature(conn, &f.environment, f.feature.id, Some(segment.id))
            .await
            .unwrap()
            .into_iter()
            .map(|v| (v.id, v.accumulator))
            .collect();
    acc.sort();
    acc
}

/// Segment override added, then removed: the identities it captured fall through to the
/// organic pool again.
async fn override_added_then_removed(conn: &mut PoolConnection<Sqlite>, picks: &[(&str, usize)]) {
    let f = setup(conn).await;
    let matching = pick(conn, &f, picks).await;

    let segment = override_segment(conn, &f, &matching).await;
    read(conn, &f.environment, &names()).await;

    apply(
        conn,
        &f.project,
        &f.environment,
        segment,
        vec![SegmentPatchOp::UnsetFeatureOverride {
            feature_id: f.feature.id,
        }],
    )
    .await;
    read(conn, &f.environment, &names()).await;

    assert_eq!(tally(&population(conn, &f).await), (50, 30, 20));
}

/// The reported case: identities that held 0 dark / 2 light / 2 auto used to end up as
/// 52 / 29 / 19.
#[sqlx::test]
async fn override_removed_after_capturing_light_and_auto(mut conn: PoolConnection<Sqlite>) {
    override_added_then_removed(&mut conn, HELD_LIGHT_AND_AUTO).await;
}

/// ...and 4 dark ones used to end up as 48 / 31 / 21.
#[sqlx::test]
async fn override_removed_after_capturing_dark(mut conn: PoolConnection<Sqlite>) {
    override_added_then_removed(&mut conn, HELD_DARK).await;
}

/// ...while a balanced share (the head of a cycle) happened to come out right regardless.
#[sqlx::test]
async fn override_removed_after_capturing_a_balanced_share(mut conn: PoolConnection<Sqlite>) {
    override_added_then_removed(&mut conn, HELD_BALANCED).await;
}

/// Identities leave a segment whose override stays in place (their rules stopped matching),
/// so the segment's own accumulators must get their draws back too - not just the organic
/// ones that they are drawn from again.
#[sqlx::test]
async fn leaving_a_segment_that_keeps_its_override(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let matching = pick(&mut conn, &f, HELD_LIGHT_AND_AUTO).await;

    let segment = override_segment(&mut conn, &f, &matching).await;
    let untouched = segment_accumulators(&mut conn, &f, &segment).await;

    read(&mut conn, &f.environment, &names()).await;
    assert_ne!(
        segment_accumulators(&mut conn, &f, &segment).await,
        untouched,
        "the 4 identities drew from the segment's accumulators"
    );

    let rule_ids: Vec<_> = segment.groups[0].rules.iter().map(|r| r.id).collect();
    apply(
        &mut conn,
        &f.project,
        &f.environment,
        segment.clone(),
        rule_ids
            .into_iter()
            .map(|rule_id| SegmentPatchOp::DeleteRule { rule_id })
            .collect(),
    )
    .await;
    read(&mut conn, &f.environment, &names()).await;

    assert_eq!(tally(&population(&mut conn, &f).await), (50, 30, 20));
    assert_eq!(
        segment_accumulators(&mut conn, &f, &segment).await,
        untouched,
        "the segment's accumulators are as if nobody was ever drawn from them"
    );
}

/// Pinned identities don't take part in distribution, so pinning hands their draws back -
/// and unpinning them lets them draw again, into exactly the slots they vacated.
#[sqlx::test]
async fn pinning_and_unpinning(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let pinned = pick(&mut conn, &f, HELD_LIGHT_AND_AUTO).await;

    for name in &pinned {
        let ident = identity::get_by_value(&mut conn, &f.environment, name.clone())
            .await
            .unwrap();
        identity::override_variant(&mut conn, &f.environment, &ident, f.feature.id, f.dark.id)
            .await
            .unwrap();
    }
    // the pins are not redistributed by reading...
    read(&mut conn, &f.environment, &names()).await;
    assert_eq!(tally(&population(&mut conn, &f).await), (54, 28, 18));

    for name in &pinned {
        let ident = identity::get_by_value(&mut conn, &f.environment, name.clone())
            .await
            .unwrap();
        SQLIdentities::delete_identity_variant_for_feature(
            &mut *conn,
            params![ident.id, f.feature.id, f.environment.id],
        )
        .await
        .unwrap();
    }
    read(&mut conn, &f.environment, &names()).await;
    assert_eq!(tally(&population(&mut conn, &f).await), (50, 30, 20));
}

/// `UNSET distribution` frees identities to be redistributed on their next read.
#[sqlx::test]
async fn clearing_distribution(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let cleared = pick(&mut conn, &f, HELD_LIGHT_AND_AUTO).await;

    for name in &cleared {
        identity::clear_distribution_for_feature(&mut conn, &f.environment, f.feature.id, name)
            .await
            .unwrap();
    }
    assert_eq!(population(&mut conn, &f).await.len(), IDENTITIES - 4);

    read(&mut conn, &f.environment, &cleared).await;
    assert_eq!(tally(&population(&mut conn, &f).await), (50, 30, 20));
}

/// Newcomers fill the slots of deleted identities instead of starting a new cycle.
#[sqlx::test]
async fn deleting_identities(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let deleted = pick(&mut conn, &f, HELD_LIGHT_AND_AUTO).await;

    for name in &deleted {
        let ident = identity::get_by_value(&mut conn, &f.environment, name.clone())
            .await
            .unwrap();
        identity::delete(&mut conn, ident).await.unwrap();
    }
    assert_eq!(population(&mut conn, &f).await.len(), IDENTITIES - 4);

    let newcomers: Vec<String> = (1..=4).map(|n| format!("newcomer-{n}")).collect();
    read(&mut conn, &f.environment, &newcomers).await;
    assert_eq!(tally(&population(&mut conn, &f).await), (50, 30, 20));
}

/// A segment matching every identity and overriding the feature's weights to
/// dark 50 / light 40 / auto 10 - i.e. only 10 of the 100 identities (auto ones, turning
/// light) actually have to change variant.
async fn override_everyone(conn: &mut PoolConnection<Sqlite>, f: &Fixture) -> Segment {
    let segment = segment::create(conn, &f.project, "everyone".to_owned(), None)
        .await
        .unwrap();
    apply(
        conn,
        &f.project,
        &f.environment,
        segment,
        vec![
            add_group(None),
            add_rule("group-1", Subject::Identity, Comparator::Contains, "user-"),
            SegmentPatchOp::SetFeatureOverride {
                feature_id: f.feature.id,
                variant_weights: vec![
                    SegmentVariantWeight {
                        variant_id: f.light.id,
                        weight: 40,
                    },
                    SegmentVariantWeight {
                        variant_id: f.auto.id,
                        weight: 10,
                    },
                ],
            },
        ],
    )
    .await
}

/// Reads the identities one by one, in the given order, and returns how many times each of
/// them changed variant along the way.
async fn read_counting_changes(
    conn: &mut PoolConnection<Sqlite>,
    f: &Fixture,
    order: &[String],
) -> HashMap<String, u32> {
    let mut changes = HashMap::new();
    let mut previous = population(conn, f).await;

    for name in order {
        read(conn, &f.environment, std::slice::from_ref(name)).await;

        let current = population(conn, f).await;
        for (identity, variant) in &current {
            if previous.get(identity) != Some(variant) {
                *changes.entry(identity.clone()).or_default() += 1;
            }
        }
        previous = current;
    }
    changes
}

/// An override moves identities to the pool's new weights - but only as many as the new
/// weights actually require. Read in the order they were first distributed, exactly the 10
/// that have to move do (a fresh draw for everybody flipped 50 of 100 here).
#[sqlx::test]
async fn overriding_weights_moves_only_the_needed_identities(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    override_everyone(&mut conn, &f).await;

    let changes = read_counting_changes(&mut conn, &f, &names()).await;

    assert_eq!(tally(&population(&mut conn, &f).await), (50, 40, 10));
    assert_eq!(changes.len(), 10);
    assert!(changes.values().all(|n| *n == 1));
}

/// Identities are read in whatever order the traffic brings them, and a pool doesn't know
/// how large it will end up while it's still filling - so some more identities than the
/// minimum move, but far fewer than a fresh draw for everybody (58 of 100 here), each moving
/// at most once, and the population still settles at the new weights.
#[sqlx::test]
async fn overriding_weights_in_random_reading_order(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    override_everyone(&mut conn, &f).await;

    let mut order = names();
    order.shuffle(&mut StdRng::seed_from_u64(7));
    let changes = read_counting_changes(&mut conn, &f, &order).await;

    assert_eq!(tally(&population(&mut conn, &f).await), (50, 40, 10));
    assert!(changes.len() <= 40, "{} identities changed", changes.len());
    assert!(changes.values().all(|n| *n == 1));

    // ...and once settled, reading again changes nothing
    assert!(
        read_counting_changes(&mut conn, &f, &names())
            .await
            .is_empty()
    );
}

/// Sets the segment's override for the feature to light / auto (dark, the control, takes
/// the remainder).
async fn edit_override(
    conn: &mut PoolConnection<Sqlite>,
    f: &Fixture,
    segment: Segment,
    light: u8,
    auto: u8,
) -> Segment {
    apply(
        conn,
        &f.project,
        &f.environment,
        segment,
        vec![SegmentPatchOp::SetFeatureOverride {
            feature_id: f.feature.id,
            variant_weights: vec![
                SegmentVariantWeight {
                    variant_id: f.light.id,
                    weight: light,
                },
                SegmentVariantWeight {
                    variant_id: f.auto.id,
                    weight: auto,
                },
            ],
        }],
    )
    .await
}

/// Assignments with a migration still waiting to be applied on the identity's next read.
async fn pending_migrations(conn: &mut PoolConnection<Sqlite>, f: &Fixture) -> usize {
    let rows: Vec<Assignment> =
        SQLIdentities::fetch_identities(&mut **conn, params![f.environment.id, f.feature.id])
            .await
            .unwrap();
    rows.iter().filter(|r| r.migrated_id.is_some()).count()
}

/// How many identities hold a different variant in `after` than in `before`.
fn changed(before: &Population, after: &Population) -> usize {
    before
        .iter()
        .filter(|(i, v)| after.get(*i) != Some(*v))
        .count()
}

/// Editing an override's weights while the segment already has members moves them to the new
/// weights - exactly as many as it takes. Members used to stay on the old weights (50 / 40 /
/// 10 here) no matter what the override said.
#[sqlx::test]
async fn editing_override_weights_moves_members_to_the_new_weights(
    mut conn: PoolConnection<Sqlite>,
) {
    let f = setup(&mut conn).await;
    let segment = override_everyone(&mut conn, &f).await; // dark 50 / light 40 / auto 10
    read(&mut conn, &f.environment, &names()).await;

    let before = population(&mut conn, &f).await;
    assert_eq!(tally(&before), (50, 40, 10));

    edit_override(&mut conn, &f, segment, 10, 50).await; // dark 40 / light 10 / auto 50

    // The effective assignment is the new one right away (migrations are lazy)...
    let edited = population(&mut conn, &f).await;
    assert_eq!(tally(&edited), (40, 10, 50));
    // ...and touches just the surplus: half the sum of the per-variant differences.
    assert_eq!(changed(&before, &edited), 40);
    assert_eq!(pending_migrations(&mut conn, &f).await, 40);

    // Reading settles the migrations without moving anybody else, keeping every identity in
    // the segment.
    read(&mut conn, &f.environment, &names()).await;
    let settled = population(&mut conn, &f).await;
    assert_eq!(settled, edited);
    assert_eq!(pending_migrations(&mut conn, &f).await, 0);
    assert_eq!(changed(&before, &settled), 40);
}

/// Identities joining a segment after an edit fill the pool's real deficits: 100 more at
/// 40 / 10 / 50 give 80 / 20 / 100 (they used to draw as if the pool were empty and land at
/// 90 / 50 / 60, a blend of the old and the new weights).
#[sqlx::test]
async fn identities_joining_after_an_edit_follow_the_new_weights(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let segment = override_everyone(&mut conn, &f).await;
    read(&mut conn, &f.environment, &names()).await;

    edit_override(&mut conn, &f, segment, 10, 50).await;
    read(&mut conn, &f.environment, &names()).await;

    let newcomers: Vec<String> = (101..=200).map(|n| format!("user-{n}")).collect();
    read(&mut conn, &f.environment, &newcomers).await;

    assert_eq!(tally(&population(&mut conn, &f).await), (80, 20, 100));
}

/// Setting the very same weights again is not an edit: nobody moves.
#[sqlx::test]
async fn resetting_the_same_weights_moves_nobody(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let segment = override_everyone(&mut conn, &f).await;
    read(&mut conn, &f.environment, &names()).await;
    let before = population(&mut conn, &f).await;

    edit_override(&mut conn, &f, segment, 40, 10).await;
    read(&mut conn, &f.environment, &names()).await;

    assert_eq!(pending_migrations(&mut conn, &f).await, 0);
    assert_eq!(population(&mut conn, &f).await, before);
}

/// A small segment, so that the targets involve rounding: 4 members on 70 / 10 / 20 become
/// 25 / 50 / 25, i.e. 1 dark, 2 light and 1 auto.
#[sqlx::test]
async fn editing_the_weights_of_a_small_segment(mut conn: PoolConnection<Sqlite>) {
    let f = setup(&mut conn).await;
    let matching = pick(&mut conn, &f, HELD_LIGHT_AND_AUTO).await;
    let segment = override_segment(&mut conn, &f, &matching).await; // dark 70 / light 10 / auto 20
    read(&mut conn, &f.environment, &names()).await;

    edit_override(&mut conn, &f, segment, 50, 25).await; // dark 25 / light 50 / auto 25

    let population = population(&mut conn, &f).await;
    let members: Population = matching
        .iter()
        .map(|name| (name.clone(), population[name]))
        .collect();
    assert_eq!(tally(&members), (1, 2, 1));
    // the other 96 are organic and didn't move: initial 50 / 30 / 20 minus the (0, 2, 2)
    // the 4 held to begin with, plus their (1, 2, 1) now
    assert_eq!(tally(&population), (51, 30, 19));
}

#[test]
fn target_counts_are_exact_shares_rounded_by_largest_remainder() {
    let weights = [(1, 50), (2, 30), (3, 20)];

    // 3.5 / 2.1 / 1.4 -> the single leftover identity goes to the largest remainder
    let targets = distributor::target_counts(&weights, 7);
    assert_eq!((targets[&1], targets[&2], targets[&3]), (4, 2, 1));

    let targets = distributor::target_counts(&weights, 100);
    assert_eq!((targets[&1], targets[&2], targets[&3]), (50, 30, 20));

    for total in 0..250 {
        let targets = distributor::target_counts(&weights, total);
        assert_eq!(targets.values().sum::<i64>(), total);
        for (id, weight) in weights {
            let exact = weight as f64 * total as f64 / 100.0;
            assert!((targets[&id] as f64 - exact).abs() < 1.0, "{total}: {id}");
        }
    }
    assert!(
        distributor::target_counts(&[(1, 0), (2, 0)], 10)
            .values()
            .all(|n| *n == 0)
    );
}
