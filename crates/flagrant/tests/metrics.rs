use flagrant::models::{
    feature,
    identity::{self, HugSql, SQLIdentities},
    metrics, segment, traits, variant,
};
use flagrant_types::{
    Comparator, Environment, Feature, Subject, VariantValue,
    payload::{SegmentPatchOp, SegmentVariantWeight},
};
use hugsqlx::params;
use sqlx::{Sqlite, pool::PoolConnection};

use crate::common::{add_group, add_rule, apply, create_context, create_environment};

mod common;

async fn create_feature(conn: &mut PoolConnection<Sqlite>, environment: &Environment) -> Feature {
    feature::create(
        conn,
        environment,
        "featuriozzo".to_owned(),
        None,
        VariantValue::build("foo"),
        true,
        false,
    )
    .await
    .unwrap()
}

/// Requests the feature on behalf of `count` fresh identities, so each ends up assigned.
async fn resolve_identities(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    count: usize,
) -> Vec<flagrant_types::Identity> {
    let mut identities = Vec::new();
    for n in 1..=count {
        let ident = identity::get_or_create_by_value(conn, environment, format!("identity_{n}"))
            .await
            .unwrap();
        identity::get_identity_variants(conn, environment, &ident)
            .await
            .unwrap();
        identities.push(ident);
    }
    identities
}

/// Identity counts per variant value of the (only) feature in `environment`.
async fn variant_counts(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
) -> Vec<(String, i64)> {
    let mut counts: Vec<_> = metrics::variant_identity_counts(conn)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.environment == environment.name)
        .map(|row| (row.variant.bare().to_owned(), row.identities))
        .collect();
    counts.sort();
    counts
}

#[sqlx::test]
async fn counts_identities_per_environment(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let empty_environment = create_environment(&mut conn, &project).await;

    create_feature(&mut conn, &environment).await;
    resolve_identities(&mut conn, &environment, 3).await;

    let totals = metrics::identity_totals(&mut conn).await.unwrap();
    let total_for = |env: &Environment| {
        totals
            .iter()
            .find(|row| row.environment == env.name)
            .map(|row| row.identities)
    };

    assert_eq!(total_for(&environment), Some(3));
    // environments without any identity are still reported, as zero
    assert_eq!(total_for(&empty_environment), Some(0));
}

#[sqlx::test]
async fn counts_follow_effective_variant(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment).await;

    // nobody assigned yet: the control variant is reported with zero
    assert_eq!(
        variant_counts(&mut conn, &environment).await,
        vec![("foo".to_owned(), 0)]
    );

    resolve_identities(&mut conn, &environment, 10).await;
    assert_eq!(
        variant_counts(&mut conn, &environment).await,
        vec![("foo".to_owned(), 10)]
    );

    // A new 50% variant only records `migrated_id` on half of the identities - their
    // `variant_id` still points at the control until they are next read. Counting by
    // `variant_id` alone would still report 10 on control.
    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("bazz"),
        50,
    )
    .await
    .unwrap();
    assert_eq!(
        variant_counts(&mut conn, &environment).await,
        vec![("bazz".to_owned(), 5), ("foo".to_owned(), 5)]
    );

    // pinning an identity to a variant moves it there too
    let identities = resolve_identities(&mut conn, &environment, 10).await;
    identity::override_variant(
        &mut conn,
        &environment,
        &identities[0],
        feature.id,
        variant.id,
    )
    .await
    .unwrap();
    let counts = variant_counts(&mut conn, &environment).await;
    assert_eq!(counts.iter().map(|(_, n)| n).sum::<i64>(), 10);

    // Deleting the variant drops the attachments of the identities that were on it (they get
    // redistributed lazily on their next read), so only the ones on control remain counted -
    // and the deleted variant itself is no longer reported.
    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();
    let remaining = variant_counts(&mut conn, &environment).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].0, "foo");
    assert!(remaining[0].1 < 10);

    resolve_identities(&mut conn, &environment, 10).await;
    assert_eq!(
        variant_counts(&mut conn, &environment).await,
        vec![("foo".to_owned(), 10)]
    );
}

#[sqlx::test]
async fn counts_identities_per_trait(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    create_feature(&mut conn, &environment).await;

    let country = traits::upsert(&mut conn, project.id, "country".to_owned())
        .await
        .unwrap();
    traits::upsert(&mut conn, project.id, "beta".to_owned())
        .await
        .unwrap();

    let identities = resolve_identities(&mut conn, &environment, 3).await;
    for (ident, value) in identities.iter().zip(["pl", "de"]) {
        SQLIdentities::upsert_identity_trait(
            &mut *conn,
            params![ident.id, country.id, value.to_owned()],
        )
        .await
        .unwrap();
    }

    let counts: Vec<_> = metrics::trait_identity_counts(&mut conn)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.environment == environment.name)
        .map(|row| (row.trait_name, row.identities))
        .collect();

    // counted per trait regardless of value, and a trait nobody carries is reported as zero
    assert_eq!(
        counts,
        vec![("beta".to_owned(), 0), ("country".to_owned(), 2)]
    );
}

/// Reads the feature on behalf of the identity, which distributes it (or, for an already
/// distributed but `segment_dirty` one, re-evaluates it against the changed segments).
async fn resolve(conn: &mut PoolConnection<Sqlite>, environment: &Environment, value: &str) {
    let ident = identity::get_or_create_by_value(conn, environment, value.to_owned())
        .await
        .unwrap();
    identity::get_identity_variants(conn, environment, &ident)
        .await
        .unwrap();
}

/// (variant value, identities) pairs of a segment's counts for the (only) feature in
/// `environment`.
async fn segment_counts(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    segment_name: &str,
) -> Vec<(String, i64)> {
    let mut counts: Vec<_> = metrics::segment_variant_identity_counts(conn)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.environment == environment.name && row.segment == segment_name)
        .map(|row| (row.variant.bare().to_owned(), row.identities))
        .collect();
    counts.sort();
    counts
}

async fn dirty_count(conn: &mut PoolConnection<Sqlite>, environment: &Environment) -> i64 {
    metrics::dirty_identity_counts(conn)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.environment == environment.name)
        .map(|row| row.identities)
        .sum()
}

#[sqlx::test]
async fn counts_segment_identities_and_dirty_backlog(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment).await;
    let alt = variant::create(
        &mut conn,
        &environment,
        &feature,
        VariantValue::build("alt"),
        40,
    )
    .await
    .unwrap();

    resolve(&mut conn, &environment, "user-vip").await;
    resolve(&mut conn, &environment, "user-other").await;

    // nothing is segment-governed yet, so nothing is reported for any segment
    assert!(
        metrics::segment_variant_identity_counts(&mut conn)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(dirty_count(&mut conn, &environment).await, 0);

    let vip = segment::create(&mut conn, &project, "vip".to_owned(), None)
        .await
        .unwrap();
    apply(
        &mut conn,
        &project,
        &environment,
        vip,
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
                variant_weights: vec![SegmentVariantWeight {
                    variant_id: alt.id,
                    weight: 100,
                }],
            },
        ],
    )
    .await;

    // A fresh override nobody has hit yet is reported as zeros (not omitted), while every
    // already-distributed identity of the feature is flagged for re-evaluation - matching
    // or not, since marking is a blanket per-feature operation.
    assert_eq!(
        segment_counts(&mut conn, &environment, "vip").await,
        vec![("alt".to_owned(), 0), ("foo".to_owned(), 0)]
    );
    assert_eq!(dirty_count(&mut conn, &environment).await, 2);

    // Reading settles the backlog, and only the matching identity ends up attributed to the
    // segment - on the variant the override sends it to.
    resolve(&mut conn, &environment, "user-vip").await;
    resolve(&mut conn, &environment, "user-other").await;

    assert_eq!(
        segment_counts(&mut conn, &environment, "vip").await,
        vec![("alt".to_owned(), 1), ("foo".to_owned(), 0)]
    );
    assert_eq!(dirty_count(&mut conn, &environment).await, 0);
}
