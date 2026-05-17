use flagrant::models::{
    feature,
    identity::{self, HugSql, SQLIdentities},
    project,
    traits,
    variant,
};
use flagrant_types::{Environment, Feature, FeatureValue, Project, TraitValue, Variant, payload::IdentityTraitPayload};
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
    project: &Project,
    environment: &Environment,
    feature: &Feature,
    variant: &Variant,
) -> usize {
    // Redistribute idents and attach to variants first
    for n in 1..=10 {
        identity::get_identity_variants(conn, project, environment, format!("identity_{n}"))
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
    let (project, environment) = create_context(&mut conn).await;
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
        identity::get_identity_variants(&mut conn, &project, &environment, format!("identity_{n}"))
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
    let (project, environment) = create_context(&mut conn).await;
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
        idents_count_for_feature_variant(&mut conn, &project, &environment, &feature, &variant)
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

    assert_eq!(
        idents_count_for_feature_variant(&mut conn, &project, &environment, &feature, &variant)
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

    assert_eq!(
        idents_count_for_feature_variant(&mut conn, &project, &environment, &feature, &variant)
            .await,
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

#[sqlx::test]
async fn create_identity_without_traits(mut conn: PoolConnection<Sqlite>) {
    let (project, _) = create_context(&mut conn).await;

    let created = identity::create(&mut conn, &project, "user_alice".to_owned(), vec![])
        .await
        .unwrap();

    assert_eq!(created.value, "user_alice");
    assert!(created.traits.is_empty());
}

#[sqlx::test]
async fn create_identity_with_traits(mut conn: PoolConnection<Sqlite>) {
    let (project, _) = create_context(&mut conn).await;

    let trait_payloads = vec![
        IdentityTraitPayload { name: "country".to_owned(), value: Some(TraitValue::Str("pl".to_owned())) },
        IdentityTraitPayload { name: "age".to_owned(), value: Some(TraitValue::Int(30)) },
    ];
    let created = identity::create(&mut conn, &project, "user_bob".to_owned(), trait_payloads)
        .await
        .unwrap();

    assert_eq!(created.value, "user_bob");
    assert_eq!(created.traits.len(), 2);

    let names: Vec<&str> = created.traits.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"country"));
    assert!(names.contains(&"age"));
}

#[sqlx::test]
async fn update_identity_traits(mut conn: PoolConnection<Sqlite>) {
    let (project, _) = create_context(&mut conn).await;

    let initial_traits = vec![
        IdentityTraitPayload { name: "country".to_owned(), value: Some(TraitValue::Str("pl".to_owned())) },
    ];
    let created = identity::create(&mut conn, &project, "user_carol".to_owned(), initial_traits)
        .await
        .unwrap();
    assert_eq!(created.traits.len(), 1);

    let stored = identity::get_by_id(&mut conn, created.id).await.unwrap();
    let new_traits = vec![
        IdentityTraitPayload { name: "tier".to_owned(), value: Some(TraitValue::Str("premium".to_owned())) },
        IdentityTraitPayload { name: "beta".to_owned(), value: Some(TraitValue::Bool(true)) },
    ];
    let updated = identity::update_traits(&mut conn, &project, stored, new_traits)
        .await
        .unwrap();

    assert_eq!(updated.traits.len(), 2);
    let names: Vec<&str> = updated.traits.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"tier"));
    assert!(names.contains(&"beta"));
    assert!(!names.contains(&"country"));
}

#[sqlx::test]
async fn delete_identity(mut conn: PoolConnection<Sqlite>) {
    let (project, _) = create_context(&mut conn).await;

    let created = identity::create(&mut conn, &project, "user_dave".to_owned(), vec![])
        .await
        .unwrap();
    let identity_id = created.id;

    let stored = identity::get_by_id(&mut conn, identity_id).await.unwrap();
    identity::delete(&mut conn, stored).await.unwrap();

    assert!(identity::get_by_id(&mut conn, identity_id).await.is_err());
}

#[sqlx::test]
async fn identities_are_scoped_to_project(mut conn: PoolConnection<Sqlite>) {
    let (project_a, _) = create_context(&mut conn).await;
    let project_b = project::create(&mut conn, "second_project".to_owned()).await.unwrap();

    identity::create(&mut conn, &project_a, "alice".to_owned(), vec![]).await.unwrap();
    identity::create(&mut conn, &project_a, "bob".to_owned(), vec![]).await.unwrap();
    identity::create(&mut conn, &project_b, "carol".to_owned(), vec![]).await.unwrap();

    let a_identities = identity::list(&mut conn, &project_a, None).await.unwrap();
    let b_identities = identity::list(&mut conn, &project_b, None).await.unwrap();

    assert_eq!(a_identities.len(), 2);
    assert_eq!(b_identities.len(), 1);

    let a_values: Vec<&str> = a_identities.iter().map(|i| i.value.as_str()).collect();
    assert!(a_values.contains(&"alice"));
    assert!(a_values.contains(&"bob"));
    assert_eq!(b_identities[0].value, "carol");
}

#[sqlx::test]
async fn traits_are_scoped_to_project(mut conn: PoolConnection<Sqlite>) {
    let (project_a, _) = create_context(&mut conn).await;
    let project_b = project::create(&mut conn, "second_project".to_owned()).await.unwrap();

    traits::upsert(&mut conn, &project_a, "country".to_owned()).await.unwrap();
    traits::upsert(&mut conn, &project_a, "tier".to_owned()).await.unwrap();
    traits::upsert(&mut conn, &project_b, "country".to_owned()).await.unwrap();

    let a_traits = traits::get_all(&mut conn, &project_a).await.unwrap();
    let b_traits = traits::get_all(&mut conn, &project_b).await.unwrap();

    assert_eq!(a_traits.len(), 2);
    assert_eq!(b_traits.len(), 1);
}

#[sqlx::test]
async fn deleting_trait_removes_it_from_identities(mut conn: PoolConnection<Sqlite>) {
    let (project, _) = create_context(&mut conn).await;

    let trait_payloads = vec![
        IdentityTraitPayload { name: "country".to_owned(), value: Some(TraitValue::Str("pl".to_owned())) },
        IdentityTraitPayload { name: "tier".to_owned(), value: Some(TraitValue::Str("free".to_owned())) },
    ];
    let created = identity::create(&mut conn, &project, "user_eve".to_owned(), trait_payloads)
        .await
        .unwrap();
    assert_eq!(created.traits.len(), 2);

    let all_traits = traits::get_all(&mut conn, &project).await.unwrap();
    let country_trait = all_traits.iter().find(|t| t.name == "country").unwrap();

    traits::delete(&mut conn, &project, country_trait.id).await.unwrap();

    let updated = identity::get_with_traits_by_id(&mut conn, created.id).await.unwrap();
    assert_eq!(updated.traits.len(), 1);
    assert_eq!(updated.traits[0].name, "tier");
}
