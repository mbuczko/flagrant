use common::{create_context, create_environment, random_string};
use flagrant::models::{feature, project, variant};
use flagrant_types::FeatureValue;
use sqlx::{pool::PoolConnection, Sqlite};

use crate::common::create_feature;

mod common;

#[sqlx::test]
async fn create_project(mut conn: PoolConnection<Sqlite>) {
    let name = "Sample project";
    let project = project::create(&mut conn, name.to_owned()).await.unwrap();

    assert_eq!(project.name, name);
}

#[sqlx::test]
async fn create_feature_with_default_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let value = FeatureValue::Json("{\"foo\": 2}".to_owned());
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        value.clone(),
        true,
    )
    .await
    .unwrap();

    assert_eq!(feature.variants.len(), 1);
    assert_eq!(feature.get_default_value(), &value);

    let default_variant = feature.get_default_variant();

    assert_eq!(default_variant.weight, 100);
    assert!(default_variant.is_control());
}

#[sqlx::test]
async fn create_feature_with_missing_default_variant_in_other_env(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment1) = create_context(&mut conn).await;
    let environment2 = create_environment(&mut conn, &project).await;
    let feature = create_feature(&mut conn, &environment1, "foo").await;

    variant::create(
        &mut conn,
        &environment1,
        &feature,
        FeatureValue::build("bar"),
        40,
    )
    .await
    .unwrap();

    // no default variant in environment2, hence list of variants is empty even though
    // some have been created in environment1.
    let feature = feature::get_by_id(&mut conn, &environment2, feature.id)
        .await
        .unwrap();
    assert!(feature.variants.is_empty());

    // after adding default variant, a list consisting of default- and previously created
    // variant should be returned.
    feature::update_one(&mut conn, &environment2, &feature)
        .value(FeatureValue::build("bazz"))
        .update()
        .await
        .unwrap();

    let feature = feature::get_by_id(&mut conn, &environment2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.variants.len(), 2);
}

#[sqlx::test]
async fn create_feature_with_different_values_in_envs(mut conn: PoolConnection<Sqlite>) {
    let (project, environment1) = create_context(&mut conn).await;
    let environment2 = create_environment(&mut conn, &project).await;
    let feature = create_feature(&mut conn, &environment1, "foo").await;

    feature::update_one(&mut conn, &environment2, &feature)
        .value(FeatureValue::build("bazz"))
        .update()
        .await
        .unwrap();

    let fv1 = FeatureValue::Text("foo".to_string());
    let fv2 = FeatureValue::Text("bazz".to_string());

    let feature = feature::get_by_id(&mut conn, &environment1, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), &fv1);

    let feature = feature::get_by_id(&mut conn, &environment2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), &fv2);
}

#[sqlx::test]
async fn create_feature_with_invalid_name(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    for name in [" ble", "123", "💕333", "foo-bazz"] {
        let feature = feature::create(
            &mut conn,
            &environment,
            name.to_owned(),
            FeatureValue::Text("foo".to_owned()),
            false,
        )
        .await;
        assert!(feature.is_err())
    }

    // name too long
    let feature = feature::create(
        &mut conn,
        &environment,
        format!("F_{}", random_string(1024)),
        FeatureValue::Text("foo".to_owned()),
        false,
    )
    .await;
    assert!(feature.is_err())
}

#[sqlx::test]
#[should_panic]
async fn create_feature_with_non_unique_name(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let name = "should_be_unique";

    feature::create(
        &mut conn,
        &environment,
        name.to_owned(),
        FeatureValue::Text("foo".to_owned()),
        false,
    )
    .await
    .unwrap();

    feature::create(
        &mut conn,
        &environment,
        name.to_owned(),
        FeatureValue::Text("foo".to_owned()),
        false,
    )
    .await
    .unwrap();
}

#[sqlx::test]
async fn delete_feature_with_default_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "foo").await;

    assert!(feature::delete(&mut conn, &environment, &feature)
        .await
        .is_ok());
    assert!(feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .is_err());
}

#[sqlx::test]
async fn delete_feature_with_variants(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "foo").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        10,
    )
    .await
    .unwrap();

    assert!(feature::delete(&mut conn, &environment, &feature)
        .await
        .is_ok());
    assert!(feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .is_err());
}

#[sqlx::test]
async fn create_valid_variant(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "foo").await;
    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar"),
        10,
    )
    .await;
    assert!(variant.is_ok());
}

#[sqlx::test]
async fn create_variants_with_valid_weights(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        30,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-3"),
        40,
    )
    .await
    .unwrap();

    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant();
    assert_eq!(default_variant.weight, 20);
}

#[sqlx::test]
async fn create_variants_with_exceeding_weight(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        30,
    )
    .await
    .unwrap();

    let exceeding_variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-3"),
        90,
    );
    assert!(exceeding_variant.await.is_err());
}

#[sqlx::test]
async fn create_variants_with_different_weights_in_envs(mut conn: PoolConnection<Sqlite>) {
    let (project, environment1) = create_context(&mut conn).await;
    let environment2 = create_environment(&mut conn, &project).await;
    let feature = create_feature(&mut conn, &environment1, "foo").await;
    let variant = variant::create(
        &mut conn,
        &environment1,
        &feature,
        FeatureValue::build("bar"),
        40,
    )
    .await
    .unwrap();

    feature::update_one(&mut conn, &environment2, &feature)
        .value(FeatureValue::build("bazz"))
        .update()
        .await
        .unwrap();

    variant::update_one(
        &mut conn,
        &environment2,
        &variant,
        FeatureValue::build("new-bar"),
        99,
    )
    .await
    .unwrap();

    let variant_env1 = variant::get_by_id(&mut conn, &environment1, variant.id)
        .await
        .unwrap();
    let variant_env2 = variant::get_by_id(&mut conn, &environment2, variant.id)
        .await
        .unwrap();

    // variant values are common across all environments
    assert_eq!(variant_env1.value, "text::new-bar".parse().unwrap());
    assert_eq!(variant_env2.value, "text::new-bar".parse().unwrap());

    // ...but weights are not
    assert_eq!(variant_env1.weight, 40);
    assert_eq!(variant_env2.weight, 99);
}

#[sqlx::test]
#[should_panic]
async fn disallow_default_variant_manual_updates(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "foo").await;
    let default_variant = feature.get_default_variant();

    variant::update_one(
        &mut conn,
        &environment,
        default_variant,
        FeatureValue::build("bar"),
        50,
    )
    .await
    .unwrap();
}

#[sqlx::test]
async fn recalculate_default_weight_for_variant_update(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        30,
    )
    .await
    .unwrap();

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-3"),
        40,
    )
    .await
    .unwrap();

    variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("new-bar-3"),
        50,
    )
    .await
    .unwrap();

    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant();
    assert_eq!(default_variant.weight, 10);
}

#[sqlx::test]
async fn recalculate_default_weight_for_variant_delete(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        30,
    )
    .await
    .unwrap();

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-3"),
        40,
    )
    .await
    .unwrap();

    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();

    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant();

    assert_eq!(default_variant.weight, 60);
}

#[sqlx::test]
async fn ignore_default_weight_recalculation_for_exceeding_weight_update(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        30,
    )
    .await
    .unwrap();

    // update with exceeding weight should fail
    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-3"),
        40,
    )
    .await
    .unwrap();

    assert!(variant::update_one(
        &mut conn,
        &environment,
        &variant,
        FeatureValue::build("new-bar-3"),
        80
    )
    .await
    .is_err());

    // default weight should retain old value
    let feature = feature::get_by_id(&mut conn, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant();
    assert_eq!(default_variant.weight, 20);
}

#[sqlx::test]
async fn disallow_removing_default_variant_when_other_variants_exist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-1"),
        10,
    )
    .await
    .unwrap();

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar-2"),
        30,
    )
    .await
    .unwrap();

    let variant = feature.get_default_variant();
    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_err());
    assert!(feature.get_default_variant().is_control())
}

#[sqlx::test]
async fn allow_removing_default_variant_when_no_other_variants_exist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, "bar").await;
    let variant = feature.get_default_variant();

    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_ok());
}
