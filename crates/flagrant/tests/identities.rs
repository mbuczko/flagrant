use flagrant::models::{
    feature,
    identity::{self, HugSql, SQLIdentities},
    variant,
};
use flagrant_types::{Environment, Feature, FeatureValue, Identity, Variant};
use hugsqlx::params;
use sqlx::{Sqlite, SqliteConnection, pool::PoolConnection};

use crate::common::create_context;

mod common;

#[derive(Debug, sqlx::FromRow)]
struct VariantMigration {
    variant_id: i32,
    migrated_id: Option<i32>,
}

async fn get_test_migrations(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<Vec<VariantMigration>> {
    Ok(SQLIdentities::fetch_identities(conn, params![environment.id, feature.id]).await?)
}

async fn migrations_count_for_feature_variant(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    feature: &Feature,
    variant_id: Option<i32>,
) -> usize {
    get_test_migrations(conn, environment, feature)
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
    // Redistribute idents and attach to variants first
    for n in 1..=10 {
        identity::get_identity_variants(conn, environment, Identity(format!("identity_{n}")))
            .await
            .unwrap();
    }

    get_test_migrations(conn, environment, feature)
        .await
        .unwrap()
        .iter()
        .filter(|i| i.variant_id == variant.id)
        .collect::<Vec<_>>()
        .len()
}

/// Smoke tests for identity migrations.
///
/// There are a few operations that impact variant weights:
///  - adding / deleting a variant (impacts the control variant weight)
///  - updating a variant weight up or down
///
/// A variant weight change always raises the question:
///
///   "What about identities already attached to altered variants
///    that exceed the new weight?"
///
/// This is where migrations kick in. The following tests verify that each
/// case is handled correctly and identities are marked as "ready-to-migrate"
/// whenever they should be redistributed to another variant on the next hit.
#[sqlx::test]
async fn migrate_identities(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        Some("descriptozzo".to_owned()),
        FeatureValue::build("foo"),
        true,
        true,
    )
    .await
    .unwrap();

    // Create identities by requesting a feature on their behalf
    for n in 1..=10 {
        identity::get_identity_variants(&mut conn, &environment, Identity(format!("identity_{n}")))
            .await
            .unwrap();
    }

    // Initially, there should be no migrated identities - all are assigned to the only (control) variant
    assert_eq!(
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, None).await,
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

    // Having a new variant created with weight=50%, half of identities should be migrated
    assert_eq!(
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, Some(variant.id))
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

    // Having the variant updated to weight=80%, 8 out of 10 identities should be migrated
    assert_eq!(
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, Some(variant.id))
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

    // Having the variant downgraded to weight=10%, 1 out of 10 identities should be migrated
    assert_eq!(
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, Some(variant.id))
            .await,
        1
    );

    let variant = variant::get_by_id(&mut conn, &environment, variant.id)
        .await
        .unwrap();

    // Having the variant deleted, all identities should be migrated back to the control variant
    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();

    assert_eq!(
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, Some(variant.id))
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
        None,
        FeatureValue::build("foo"),
        true,
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
        migrations_count_for_feature_variant(&mut conn, &environment, &feature, Some(variant.id))
            .await,
        0
    );
}
