use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{Environment, Feature, FeatureValue, Project};
use rand::Rng;
use sqlx::{pool::PoolConnection, Sqlite};

const KEY_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                             abcdefghijklmnopqrstuvwxyz\
                             0123456789_";

pub fn random_string(len: u16) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..KEY_CHARSET.len() - 1);
            KEY_CHARSET[idx] as char
        })
        .collect()
}

async fn create_environment(conn: &mut PoolConnection<Sqlite>, project: &Project) -> Environment {
    environment::create(
        conn,
        project,
        format!("ENV_{}", random_string(32)),
        Some("Lorem ipsum".to_owned()),
    )
    .await
    .unwrap()
}

async fn create_context(conn: &mut PoolConnection<Sqlite>) -> (Project, Environment) {
    let project = project::create(conn, "fancy project".to_owned())
        .await
        .unwrap();
    let environment = create_environment(conn, &project).await;

    (project, environment)
}

async fn create_feature(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    value: Option<&str>,
) -> Feature {
    feature::create(
        conn,
        environment,
        format!("F_{}", random_string(10)),
        value.map(|v| FeatureValue::Text(v.to_owned())),
        true,
    )
    .await
    .unwrap()
}

#[sqlx::test]
async fn create_project(mut conn: PoolConnection<Sqlite>) {
    let name = "Sample project";
    let project = project::create(&mut conn, name.to_owned()).await.unwrap();

    assert_eq!(project.name, name);
}

#[sqlx::test]
async fn create_feature_with_no_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = feature::create(&mut conn, &environment, "sample".to_owned(), None, true)
        .await
        .unwrap();

    assert!(feature.is_enabled);
    assert!(feature.variants.is_empty());
}

#[sqlx::test]
async fn create_feature_with_default_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let value = FeatureValue::Json("{\"foo\": 2}".to_owned());
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        Some(value.clone()),
        true,
    )
    .await
    .unwrap();

    assert_eq!(feature.variants.len(), 1);
    assert_eq!(feature.get_default_value(), Some(&value));

    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 100);
    assert!(default_variant.is_control());
}

#[sqlx::test]
async fn create_feature_with_missing_default_variant_in_other_env(
    mut conn: PoolConnection<Sqlite>,
) {
    let (project, environment1) = create_context(&mut conn).await;
    let environment2 = create_environment(&mut conn, &project).await;
    let feature = create_feature(&mut conn, &environment1, Some("foo")).await;

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
    variant::upsert_default(
        &mut conn,
        &environment2,
        &feature,
        FeatureValue::build("bazz"),
    )
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
    let feature = create_feature(&mut conn, &environment1, Some("foo")).await;

    variant::upsert_default(
        &mut conn,
        &environment2,
        &feature,
        FeatureValue::build("bazz"),
    )
    .await
    .unwrap();

    let fv1 = FeatureValue::Text("foo".to_string());
    let fv2 = FeatureValue::Text("bazz".to_string());

    let feature = feature::get_by_id(&mut conn, &environment1, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), Some(&fv1));

    let feature = feature::get_by_id(&mut conn, &environment2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), Some(&fv2));
}

#[sqlx::test]
async fn create_feature_with_invalid_name(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    for name in [" ble", "123", "💕333", "foo-bazz"] {
        let feature = feature::create(
            &mut conn,
            &environment,
            name.to_owned(),
            Some(FeatureValue::Text("foo".to_owned())),
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
        Some(FeatureValue::Text("foo".to_owned())),
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
        Some(FeatureValue::Text("foo".to_owned())),
        false,
    )
    .await
    .unwrap();

    feature::create(
        &mut conn,
        &environment,
        name.to_owned(),
        Some(FeatureValue::Text("foo".to_owned())),
        false,
    )
    .await
    .unwrap();
}

#[sqlx::test]
async fn delete_feature_with_default_value(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("foo")).await;

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
    let feature = create_feature(&mut conn, &environment, Some("foo")).await;

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
    let feature = create_feature(&mut conn, &environment, Some("foo")).await;
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
async fn create_variant_for_feature_with_no_default_variant(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, None).await;
    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bar"),
        10,
    )
    .await;
    assert!(variant.is_err());
}

#[sqlx::test]
async fn create_variants_with_valid_weights(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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
    let default_variant = feature.get_default_variant().unwrap();
    assert_eq!(default_variant.weight, 20);
}

#[sqlx::test]
async fn create_variants_with_exceeding_weight(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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
    let feature = create_feature(&mut conn, &environment1, Some("foo")).await;
    let variant = variant::create(
        &mut conn,
        &environment1,
        &feature,
        FeatureValue::build("bar"),
        40,
    )
    .await
    .unwrap();

    variant::upsert_default(
        &mut conn,
        &environment2,
        &feature,
        FeatureValue::build("bazz"),
    )
    .await
    .unwrap();

    variant::update(
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
    let feature = create_feature(&mut conn, &environment, Some("foo")).await;
    let default_variant = feature.get_default_variant().unwrap();

    variant::update(
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
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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

    variant::update(
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
    let default_variant = feature.get_default_variant().unwrap();
    assert_eq!(default_variant.weight, 10);
}

#[sqlx::test]
async fn recalculate_default_weight_for_variant_delete(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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
    let default_variant = feature.get_default_variant().unwrap();
    assert_eq!(default_variant.weight, 60);
}

#[sqlx::test]
async fn ignore_default_weight_recalculation_for_exceeding_weight_update(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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

    assert!(variant::update(
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
    let default_variant = feature.get_default_variant().unwrap();
    assert_eq!(default_variant.weight, 20);
}

#[sqlx::test]
async fn disallow_removing_default_variant_when_other_variants_exist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;

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

    let variant = feature.get_default_variant().unwrap();
    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_err());
    assert!(feature.get_default_variant().is_some())
}

#[sqlx::test]
async fn allow_removing_default_variant_when_no_other_variants_exist(
    mut conn: PoolConnection<Sqlite>,
) {
    let (_, environment) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &environment, Some("bar")).await;
    let variant = feature.get_default_variant().unwrap();

    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_ok());
}
