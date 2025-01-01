use flagrant::models::{feature, identity, variant};
use flagrant_types::{Environment, Feature, FeatureValue, Variant};
use sqlx::{pool::PoolConnection, Sqlite};

use crate::common::create_context;

mod common;

async fn migrations_count_for_feature_variant_id(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    variant_id: Option<i32>,
) -> usize {
    identity::get_identities(conn, environment, feature)
        .await
        .unwrap()
        .iter()
        .filter(|i| i.migrated_id == variant_id)
        .collect::<Vec<_>>()
        .len()
}

async fn idents_count_for_feature_variant(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    variant: &Variant,
) -> usize {
    // redistribute idents and attach to variants first
    for n in 1..=10 {
        identity::get_variants(conn, environment, format!("identity_{n}"))
            .await
            .unwrap();
    }

    identity::get_identities(conn, environment, feature)
        .await
        .unwrap()
        .iter()
        .filter(|i| i.variant_id == variant.id)
        .collect::<Vec<_>>()
        .len()
}

/// Smoke tests for identities migrations.
///
/// There are couple of operations which impact variants weights, like:
///  - adding / deleting variant (impacts control variant weight)
///  - updating variant weight up or down
///
/// Variant weight change always triggers a question:
///
///   "what about identities already attached to altered variants
///    which exceed a new weight?"
///
/// This is where migrations kick in. Following tests verify that each
/// case is handled correctly and identities are marked as "read-to-migrate"
/// whenever they should be distributed to other variant on the next hit.
#[sqlx::test]
async fn migrate_identities(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        FeatureValue::build("foo"),
        true,
    )
    .await
    .unwrap();

    // create identities by requesting a feature on their behalf
    for n in 1..=10 {
        identity::get_variants(&mut conn, &environment, format!("identity_{n}"))
            .await
            .unwrap();
    }

    // initially, there should be no migrated identities - all are assigned to the only (control) variant
    assert_eq!(
        migrations_count_for_feature_variant_id(&mut conn, &environment, &feature, None).await,
        10
    );

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bazz"),
        50,
    )
    .await
    .unwrap();

    // having new variant created with weight=50% half of identities should be migrated,
    assert_eq!(
        migrations_count_for_feature_variant_id(
            &mut conn,
            &environment,
            &feature,
            Some(variant.id)
        )
        .await,
        5
    );

    variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("buzz"),
        80,
    )
    .await
    .unwrap();

    // having new variant updated to weight=80% 8 out of 10 identities should be migrated
    assert_eq!(
        migrations_count_for_feature_variant_id(
            &mut conn,
            &environment,
            &feature,
            Some(variant.id)
        )
        .await,
        8
    );

    let variant = variant::get_by_id(&mut conn, &environment, variant.id)
        .await
        .unwrap();

    variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("bezz"),
        10,
    )
    .await
    .unwrap();

    // having new variant downgraded to weight=10% 1 out of 10 identities should be migrated
    assert_eq!(
        migrations_count_for_feature_variant_id(
            &mut conn,
            &environment,
            &feature,
            Some(variant.id)
        )
        .await,
        1
    );

    let variant = variant::get_by_id(&mut conn, &environment, variant.id)
        .await
        .unwrap();

    // having variant deleted, all identities should be migrated back to contol variant
    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();

    assert_eq!(
        migrations_count_for_feature_variant_id(
            &mut conn,
            &environment,
            &feature,
            Some(variant.id)
        )
        .await,
        0
    );
}

#[sqlx::test]
async fn distribute_identities(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        FeatureValue::build("foo"),
        true,
    )
    .await
    .unwrap();

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bazz"),
        50,
    )
    .await
    .unwrap();

    assert_eq!(
        idents_count_for_feature_variant(&mut conn, &environment, &feature, &variant).await,
        5
    );

    variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("buzz"),
        80,
    )
    .await
    .unwrap();

    assert_eq!(
        idents_count_for_feature_variant(&mut conn, &environment, &feature, &variant).await,
        8
    );

    let variant = variant::get_by_id(&mut conn, &environment, variant.id)
        .await
        .unwrap();

    variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("bezz"),
        10,
    )
    .await
    .unwrap();

    assert_eq!(
        idents_count_for_feature_variant(&mut conn, &environment, &feature, &variant).await,
        1
    );

    let variant = variant::get_by_id(&mut conn, &environment, variant.id)
        .await
        .unwrap();

    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();

    assert_eq!(
        migrations_count_for_feature_variant_id(
            &mut conn,
            &environment,
            &feature,
            Some(variant.id)
        )
        .await,
        0
    );
}
