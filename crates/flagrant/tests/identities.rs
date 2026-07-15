use flagrant::models::{
    feature,
    identity::{self, HugSql, SQLIdentities, TraitCondition},
    project, traits, variant,
};
use flagrant_types::{
    Environment, Feature, FeatureValue, TraitValue, Variant, payload::IdentityTraitPayload,
};
use hugsqlx::params;
use smallvec::smallvec;
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
        let ident = identity::get_or_create_by_value(conn, environment, format!("identity_{n}"))
            .await
            .unwrap();
        identity::get_identity_variants(conn, environment, &ident)
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
    )
    .await
    .unwrap();

    // Create identities by requesting a feature on their behalf
    for n in 1..=10 {
        let ident =
            identity::get_or_create_by_value(&mut conn, &environment, format!("identity_{n}"))
                .await
                .unwrap();
        identity::get_identity_variants(&mut conn, &environment, &ident)
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

    let variant = variant::get_by_id(&mut conn, &environment, variant.id, None)
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

    let variant = variant::get_by_id(&mut conn, &environment, variant.id, None)
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

    let variant = variant::get_by_id(&mut conn, &environment, variant.id, None)
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

    let variant = variant::get_by_id(&mut conn, &environment, variant.id, None)
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
    let (_, environment) = create_context(&mut conn).await;

    let created = identity::create(&mut conn, &environment, "user_alice".to_owned(), vec![])
        .await
        .unwrap();

    assert_eq!(created.value, "user_alice");
    assert!(created.traits.is_empty());
}

#[sqlx::test]
async fn create_identity_with_traits(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    let trait_payloads = vec![
        IdentityTraitPayload {
            name: "country".to_owned(),
            value: Some(TraitValue::Str("pl".to_owned())),
        },
        IdentityTraitPayload {
            name: "age".to_owned(),
            value: Some(TraitValue::Int(30)),
        },
    ];
    let created = identity::create(
        &mut conn,
        &environment,
        "user_bob".to_owned(),
        trait_payloads,
    )
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
    let (_, environment) = create_context(&mut conn).await;

    let initial_traits = vec![IdentityTraitPayload {
        name: "country".to_owned(),
        value: Some(TraitValue::Str("pl".to_owned())),
    }];
    let created = identity::create(
        &mut conn,
        &environment,
        "user_carol".to_owned(),
        initial_traits,
    )
    .await
    .unwrap();
    assert_eq!(created.traits.len(), 1);

    let stored = identity::get_by_value(&mut conn, &environment, created.value)
        .await
        .unwrap();
    let new_traits = vec![
        IdentityTraitPayload {
            name: "tier".to_owned(),
            value: Some(TraitValue::Str("premium".to_owned())),
        },
        IdentityTraitPayload {
            name: "beta".to_owned(),
            value: Some(TraitValue::Bool(true)),
        },
    ];
    let updated = identity::update_traits(&mut conn, &environment, stored, new_traits)
        .await
        .unwrap();

    assert_eq!(updated.traits.len(), 2);
    let names: Vec<&str> = updated.traits.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"tier"));
    assert!(names.contains(&"beta"));
    assert!(!names.contains(&"country"));
}

#[sqlx::test]
async fn override_variant_pins_identity_to_chosen_variant(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "override_feature".to_owned(),
        None,
        FeatureValue::build("control_value"),
        true,
    )
    .await
    .unwrap();

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt_value"),
        50,
    )
    .await
    .unwrap();

    // Distribute identity naturally first (lands on whichever variant the distributor picks)
    let alice = identity::get_or_create_by_value(&mut conn, &environment, "alice".to_owned())
        .await
        .unwrap();
    identity::get_identity_variants(&mut conn, &environment, &alice)
        .await
        .unwrap();

    // No override yet → get_variant_for_identity may return Some (naturally distributed) or None
    // (before first request) - either way we check after the override below.

    // Pin alice to the non-control variant explicitly
    identity::override_variant(&mut conn, &environment, &alice, feature.id, variant.id)
        .await
        .unwrap();

    // get_variant_for_identity must return the overridden variant_id
    let pinned = identity::get_variant_for_identity(&mut conn, &environment, feature.id, &alice)
        .await
        .unwrap();
    assert_eq!(
        pinned,
        Some(variant.id),
        "override should pin alice to the chosen variant"
    );

    // get_identity_variants should now surface the overridden value, not redistribute
    let iv = identity::get_identity_variants(&mut conn, &environment, &alice)
        .await
        .unwrap();
    let alice_variant = iv.iter().find(|iv| iv.feature_id == feature.id).unwrap();
    assert_eq!(
        alice_variant.feature_value,
        Some(FeatureValue::build("alt_value")),
        "get_identity_variants should return the overridden value"
    );
}

#[sqlx::test]
async fn override_variant_works_without_prior_distribution(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "override_feature2".to_owned(),
        None,
        FeatureValue::build("default"),
        true,
    )
    .await
    .unwrap();

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("special"),
        30,
    )
    .await
    .unwrap();

    // Override without any prior distribution (no identity_variants row yet)
    let bob = identity::get_or_create_by_value(&mut conn, &environment, "bob".to_owned())
        .await
        .unwrap();
    identity::override_variant(&mut conn, &environment, &bob, feature.id, variant.id)
        .await
        .unwrap();
    let pinned = identity::get_variant_for_identity(&mut conn, &environment, feature.id, &bob)
        .await
        .unwrap();
    assert_eq!(pinned, Some(variant.id));
}

#[sqlx::test]
async fn pinned_identity_not_redistributed_on_weight_change(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "pinned_feature".to_owned(),
        None,
        FeatureValue::build("control"),
        true,
    )
    .await
    .unwrap();

    let alt_variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("alt"),
        50,
    )
    .await
    .unwrap();

    // Distribute 10 identities - roughly half will land on each variant
    for n in 1..=10 {
        let ident = identity::get_or_create_by_value(&mut conn, &environment, format!("ident_{n}"))
            .await
            .unwrap();
        identity::get_identity_variants(&mut conn, &environment, &ident)
            .await
            .unwrap();
    }

    // Distribute alice and then pin her explicitly to the alt variant
    let alice = identity::get_or_create_by_value(&mut conn, &environment, "alice".to_owned())
        .await
        .unwrap();
    identity::get_identity_variants(&mut conn, &environment, &alice)
        .await
        .unwrap();
    identity::override_variant(&mut conn, &environment, &alice, feature.id, alt_variant.id)
        .await
        .unwrap();

    // Drop alt variant weight to 0 - this migrates all non-pinned identities away from it
    let alt_variant = variant::get_by_id(&mut conn, &environment, alt_variant.id, None)
        .await
        .unwrap();
    variant::update_one(
        &mut conn,
        &environment,
        &alt_variant,
        FeatureValue::build("alter"),
        0,
    )
    .await
    .unwrap();

    // alice is pinned - get_identity_variants must NOT redistribute her
    let iv = identity::get_identity_variants(&mut conn, &environment, &alice)
        .await
        .unwrap();
    let alice_iv = iv.iter().find(|iv| iv.feature_id == feature.id).unwrap();

    assert_eq!(
        alice_iv.variant_id,
        Some(alt_variant.id),
        "pinned identity should remain on the pinned variant despite weight dropping to 0"
    );
}

#[sqlx::test]
async fn delete_identity(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    let created = identity::create(&mut conn, &environment, "user_dave".to_owned(), vec![])
        .await
        .unwrap();
    let identity = created.value;
    let stored = identity::get_by_value(&mut conn, &environment, identity.clone())
        .await
        .unwrap();
    identity::delete(&mut conn, stored).await.unwrap();

    assert!(
        identity::get_by_value(&mut conn, &environment, identity)
            .await
            .is_err()
    );
}

/// `clear_matching` should delete only identities (traits included) matching the given
/// LIKE pattern within the given environment - leaving non-matching identities and
/// identities in other environments untouched.
#[sqlx::test]
async fn clear_matching_deletes_only_pattern_matches_within_environment(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, env_a) = create_context(&mut conn).await;
    let env_b = crate::common::create_environment(&mut conn, &project).await;

    identity::create(
        &mut conn,
        &env_a,
        "tester-1".to_owned(),
        vec![IdentityTraitPayload {
            name: "plan".to_owned(),
            value: Some(TraitValue::Str("premium".to_owned())),
        }],
    )
    .await
    .unwrap();
    identity::create(&mut conn, &env_a, "tester-2".to_owned(), vec![])
        .await
        .unwrap();
    identity::create(&mut conn, &env_a, "someone-else".to_owned(), vec![])
        .await
        .unwrap();
    identity::create(&mut conn, &env_b, "tester-1".to_owned(), vec![])
        .await
        .unwrap();

    identity::clear_matching(&mut conn, &env_a, "tester-%")
        .await
        .unwrap();

    assert!(
        identity::get_by_value(&mut conn, &env_a, "tester-1".to_owned())
            .await
            .is_err()
    );
    assert!(
        identity::get_by_value(&mut conn, &env_a, "tester-2".to_owned())
            .await
            .is_err()
    );
    assert!(
        identity::get_by_value(&mut conn, &env_a, "someone-else".to_owned())
            .await
            .is_ok(),
        "non-matching identity should survive"
    );
    assert!(
        identity::get_by_value(&mut conn, &env_b, "tester-1".to_owned())
            .await
            .is_ok(),
        "identity with the same value in a different environment should survive"
    );
}

/// `clear_distribution_for_feature` should remove variant assignments only for the given
/// feature and only for identities matching the LIKE pattern - leaving non-matching
/// identities' assignments, other features' assignments, and the identities themselves intact.
#[sqlx::test]
async fn clear_distribution_for_feature_removes_only_matching_assignments(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;

    let feature_a = feature::create(
        &mut conn,
        &environment,
        "feature_a".to_owned(),
        None,
        FeatureValue::build("control"),
        true,
    )
    .await
    .unwrap();
    let variant_a = variant::create(
        &mut conn,
        &environment,
        &feature_a,
        FeatureValue::build("alt"),
        50,
    )
    .await
    .unwrap();

    let feature_b = feature::create(
        &mut conn,
        &environment,
        "feature_b".to_owned(),
        None,
        FeatureValue::build("control"),
        true,
    )
    .await
    .unwrap();
    let variant_b = variant::create(
        &mut conn,
        &environment,
        &feature_b,
        FeatureValue::build("alt"),
        50,
    )
    .await
    .unwrap();

    let tester_1 = identity::get_or_create_by_value(&mut conn, &environment, "tester-1".to_owned())
        .await
        .unwrap();
    let tester_2 = identity::get_or_create_by_value(&mut conn, &environment, "tester-2".to_owned())
        .await
        .unwrap();
    let someone_else =
        identity::get_or_create_by_value(&mut conn, &environment, "someone-else".to_owned())
            .await
            .unwrap();

    // Pin tester-1, tester-2 and someone-else to variant_a for feature_a, plus tester-1 to
    // variant_b for feature_b.
    identity::override_variant(
        &mut conn,
        &environment,
        &tester_1,
        feature_a.id,
        variant_a.id,
    )
    .await
    .unwrap();
    identity::override_variant(
        &mut conn,
        &environment,
        &tester_2,
        feature_a.id,
        variant_a.id,
    )
    .await
    .unwrap();
    identity::override_variant(
        &mut conn,
        &environment,
        &someone_else,
        feature_a.id,
        variant_a.id,
    )
    .await
    .unwrap();
    identity::override_variant(
        &mut conn,
        &environment,
        &tester_1,
        feature_b.id,
        variant_b.id,
    )
    .await
    .unwrap();

    identity::clear_distribution_for_feature(&mut conn, &environment, feature_a.id, "tester-%")
        .await
        .unwrap();

    assert_eq!(
        identity::get_variant_for_identity(&mut conn, &environment, feature_a.id, &tester_1)
            .await
            .unwrap(),
        None,
        "matching identity's assignment for feature_a should be cleared"
    );
    assert_eq!(
        identity::get_variant_for_identity(&mut conn, &environment, feature_a.id, &tester_2)
            .await
            .unwrap(),
        None,
        "matching identity's assignment for feature_a should be cleared"
    );
    assert_eq!(
        identity::get_variant_for_identity(&mut conn, &environment, feature_a.id, &someone_else)
            .await
            .unwrap(),
        Some(variant_a.id),
        "non-matching identity should keep its assignment"
    );
    assert_eq!(
        identity::get_variant_for_identity(&mut conn, &environment, feature_b.id, &tester_1)
            .await
            .unwrap(),
        Some(variant_b.id),
        "matching identity's assignment for a different feature should survive"
    );
    assert!(
        identity::get_by_value(&mut conn, &environment, "tester-1".to_owned())
            .await
            .is_ok(),
        "identity itself should not be deleted"
    );
}

#[sqlx::test]
async fn identities_are_scoped_to_environment(mut conn: PoolConnection<Sqlite>) {
    let (_project_a, env_a) = create_context(&mut conn).await;
    let project_b = project::create(&mut conn, "second_project".to_owned())
        .await
        .unwrap();
    let env_b = crate::common::create_environment(&mut conn, &project_b).await;

    identity::create(&mut conn, &env_a, "alice".to_owned(), vec![])
        .await
        .unwrap();
    identity::create(&mut conn, &env_a, "bob".to_owned(), vec![])
        .await
        .unwrap();
    identity::create(&mut conn, &env_b, "carol".to_owned(), vec![])
        .await
        .unwrap();

    let a_identities = identity::list(&mut conn, &env_a, None, None, None)
        .await
        .unwrap();
    let b_identities = identity::list(&mut conn, &env_b, None, None, None)
        .await
        .unwrap();

    assert_eq!(a_identities.len(), 2);
    assert_eq!(b_identities.len(), 1);

    let a_values: Vec<&str> = a_identities.iter().map(|i| i.value.as_str()).collect();
    assert!(a_values.contains(&"alice"));
    assert!(a_values.contains(&"bob"));
    assert_eq!(b_identities[0].value, "carol");

    // Same identity value in a different environment is independent
    identity::create(&mut conn, &env_b, "alice".to_owned(), vec![])
        .await
        .unwrap();
    let b_identities = identity::list(&mut conn, &env_b, None, None, None)
        .await
        .unwrap();
    assert_eq!(b_identities.len(), 2);
    let a_identities = identity::list(&mut conn, &env_a, None, None, None)
        .await
        .unwrap();
    assert_eq!(
        a_identities.len(),
        2,
        "env_a should still have only 2 identities"
    );
}

#[sqlx::test]
async fn traits_are_scoped_to_project(mut conn: PoolConnection<Sqlite>) {
    let (project_a, _) = create_context(&mut conn).await;
    let project_b = project::create(&mut conn, "second_project".to_owned())
        .await
        .unwrap();

    traits::upsert(&mut conn, project_a.id, "country".to_owned())
        .await
        .unwrap();
    traits::upsert(&mut conn, project_a.id, "tier".to_owned())
        .await
        .unwrap();
    traits::upsert(&mut conn, project_b.id, "country".to_owned())
        .await
        .unwrap();

    let a_traits = traits::get_all(&mut conn, project_a.id).await.unwrap();
    let b_traits = traits::get_all(&mut conn, project_b.id).await.unwrap();

    assert_eq!(a_traits.len(), 2);
    assert_eq!(b_traits.len(), 1);
}

#[sqlx::test]
async fn deleting_trait_removes_it_from_identities(mut conn: PoolConnection<Sqlite>) {
    let (project, environment) = create_context(&mut conn).await;

    let trait_payloads = vec![
        IdentityTraitPayload {
            name: "country".to_owned(),
            value: Some(TraitValue::Str("pl".to_owned())),
        },
        IdentityTraitPayload {
            name: "tier".to_owned(),
            value: Some(TraitValue::Str("free".to_owned())),
        },
    ];
    let created = identity::create(
        &mut conn,
        &environment,
        "user_eve".to_owned(),
        trait_payloads,
    )
    .await
    .unwrap();
    assert_eq!(created.traits.len(), 2);

    let all_traits = traits::get_all(&mut conn, project.id).await.unwrap();
    let country_trait = all_traits.iter().find(|t| t.name == "country").unwrap();

    traits::delete(&mut conn, country_trait.id).await.unwrap();

    let updated = identity::get_by_value_with_traits(&mut conn, &environment, created.value)
        .await
        .unwrap();
    assert_eq!(updated.traits.len(), 1);
    assert_eq!(updated.traits[0].name, "tier");
}

/// Regression test: when an identity is pinned to a non-control variant,
/// `list_variant_assignments` must return the pinned variant's value and id -
/// not the control variant's values.
///
/// The original `fetch_variants_for_identity` query used `GROUP BY f.feature_id`
/// with two `max()` aggregates while leaving `v.value` and `v.variant_id`
/// unaggregated. SQLite picks unaggregated columns from an arbitrary row in the
/// group, which may be the control variant rather than the pinned one, making
/// `IDENTITY describe` show the control value as if nothing changed.
#[sqlx::test]
async fn list_variant_assignments_returns_pinned_variant_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    // Feature with a control value and a 0%-weight non-control variant.
    let feature = feature::create(
        &mut conn,
        &environment,
        "pin_display_feature".to_owned(),
        None,
        FeatureValue::build("control_value"),
        true,
    )
    .await
    .unwrap();

    let zero_pct_variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("pinned_value"),
        0,
    )
    .await
    .unwrap();

    let alice = identity::get_or_create_by_value(&mut conn, &environment, "alice".to_owned())
        .await
        .unwrap();

    identity::override_variant(
        &mut conn,
        &environment,
        &alice,
        feature.id,
        zero_pct_variant.id,
    )
    .await
    .unwrap();

    // list_variant_assignments is what IDENTITY describe calls - it must surface the
    // pinned variant's value, not fall back to the control variant.
    let assignments = identity::list_variant_assignments(&mut conn, &environment, &alice)
        .await
        .unwrap();

    let feat_iv = assignments
        .iter()
        .find(|iv| iv.feature_id == feature.id)
        .expect("feature should appear in variant assignments");

    assert_eq!(
        feat_iv.identity_id,
        Some(alice.id),
        "identity_id must be set for a pinned identity"
    );
    assert!(
        feat_iv.pinned_at.is_some(),
        "pinned_at must be set after an explicit override"
    );
    assert_eq!(
        feat_iv.feature_value,
        Some(FeatureValue::build("pinned_value")),
        "feature_value must reflect the pinned variant, not the control"
    );
    assert_eq!(
        feat_iv.variant_id,
        Some(zero_pct_variant.id),
        "variant_id must point to the pinned variant"
    );
}

#[sqlx::test]
async fn list_filters_by_included_and_excluded_traits(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    identity::create(
        &mut conn,
        &environment,
        "alice".to_owned(),
        vec![
            IdentityTraitPayload {
                name: "vip".to_owned(),
                value: Some(TraitValue::build("true")),
            },
            IdentityTraitPayload {
                name: "churned".to_owned(),
                value: Some(TraitValue::build("true")),
            },
        ],
    )
    .await
    .unwrap();

    identity::create(
        &mut conn,
        &environment,
        "bob".to_owned(),
        vec![IdentityTraitPayload {
            name: "vip".to_owned(),
            value: Some(TraitValue::build("true")),
        }],
    )
    .await
    .unwrap();

    identity::create(&mut conn, &environment, "carol".to_owned(), vec![])
        .await
        .unwrap();

    // Excluding "churned" should drop only alice, keeping bob and carol.
    let results = identity::list(
        &mut conn,
        &environment,
        None,
        None,
        Some(smallvec![TraitCondition::any_value("churned")]),
    )
    .await
    .unwrap();
    let values: Vec<_> = results.iter().map(|i| i.value.clone()).collect();
    assert!(values.contains(&"bob".to_string()));
    assert!(values.contains(&"carol".to_string()));
    assert!(!values.contains(&"alice".to_string()));

    // Including "vip" should return only alice and bob.
    let results = identity::list(
        &mut conn,
        &environment,
        None,
        Some(smallvec![TraitCondition::any_value("vip")]),
        None,
    )
    .await
    .unwrap();
    let values: Vec<_> = results.iter().map(|i| i.value.clone()).collect();
    assert!(values.contains(&"alice".to_string()));
    assert!(values.contains(&"bob".to_string()));
    assert!(!values.contains(&"carol".to_string()));
}

#[sqlx::test]
async fn list_filters_by_trait_value_with_type_coercion(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    // alice's trait is stored typed as bool, bob's as an explicit string - both should be
    // found by `trait:experimental=true` since the raw value coerces to either type.
    identity::create(
        &mut conn,
        &environment,
        "alice".to_owned(),
        vec![IdentityTraitPayload {
            name: "experimental".to_owned(),
            value: Some(TraitValue::Bool(true)),
        }],
    )
    .await
    .unwrap();

    identity::create(
        &mut conn,
        &environment,
        "bob".to_owned(),
        vec![IdentityTraitPayload {
            name: "experimental".to_owned(),
            value: Some(TraitValue::Str("true".to_owned())),
        }],
    )
    .await
    .unwrap();

    identity::create(
        &mut conn,
        &environment,
        "carol".to_owned(),
        vec![IdentityTraitPayload {
            name: "experimental".to_owned(),
            value: Some(TraitValue::Bool(false)),
        }],
    )
    .await
    .unwrap();

    let results = identity::list(
        &mut conn,
        &environment,
        None,
        Some(smallvec![TraitCondition::value("experimental", "true")]),
        None,
    )
    .await
    .unwrap();
    let values: Vec<_> = results.iter().map(|i| i.value.clone()).collect();
    assert!(values.contains(&"alice".to_string()));
    assert!(values.contains(&"bob".to_string()));
    assert!(!values.contains(&"carol".to_string()));

    // Excluding experimental=true should drop alice and bob, keeping carol.
    let results = identity::list(
        &mut conn,
        &environment,
        None,
        None,
        Some(smallvec![TraitCondition::value("experimental", "true")]),
    )
    .await
    .unwrap();
    let values: Vec<_> = results.iter().map(|i| i.value.clone()).collect();
    assert!(values.contains(&"carol".to_string()));
    assert!(!values.contains(&"alice".to_string()));
    assert!(!values.contains(&"bob".to_string()));
}

#[sqlx::test]
async fn trait_name_with_unsafe_characters_is_rejected(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    // A trait name crafted to break out of the JSON blob `identity::list` builds to
    // filter by trait (see the `identity::list` doc comment) must be rejected outright.
    let result = identity::create(
        &mut conn,
        &environment,
        "alice".to_owned(),
        vec![IdentityTraitPayload {
            name: "nope\",null],[\"vip".to_owned(),
            value: Some(TraitValue::Bool(true)),
        }],
    )
    .await;

    assert!(result.is_err());

    // The rejected trait must not have been persisted (validated inside a transaction
    // that rolls back rather than after an already-committed write).
    let traits = traits::get_all(&mut conn, environment.project_id)
        .await
        .unwrap();
    assert!(traits.is_empty());
}

#[sqlx::test]
async fn trait_value_with_unsafe_characters_is_rejected(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;

    let result = identity::create(
        &mut conn,
        &environment,
        "alice".to_owned(),
        vec![IdentityTraitPayload {
            name: "country".to_owned(),
            value: Some(TraitValue::Str("pl\",null],[\"vip".to_owned())),
        }],
    )
    .await;

    assert!(result.is_err());
}
