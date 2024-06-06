use flagrant::models::{environment, feature, project, variant};
use flagrant_types::{Environment, Feature, FeatureValue, FeatureValueType, Project};
use rand::Rng;
use sqlx::SqlitePool;

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

async fn create_environment(pool: &SqlitePool, project: &Project) -> Environment {
    environment::create(
        pool,
        project,
        format!("ENV_{}", random_string(32)),
        Some("Lorem ipsum".into()),
    )
    .await
    .unwrap()
}

async fn create_context(pool: &SqlitePool) -> (Project, Environment) {
    let project = project::create(pool, "fancy project".into()).await.unwrap();
    let environment = create_environment(pool, &project).await;

    (project, environment)
}

async fn create_feature(
    pool: &SqlitePool,
    environment: &Environment,
    value: Option<&str>,
) -> Feature {
    feature::create(
        pool,
        environment,
        format!("F_{}", random_string(10)),
        value.map(|v| FeatureValue(v.into(), FeatureValueType::Text)),
        true,
    )
    .await
    .unwrap()
}

#[flagrant::test]
async fn create_project(pool: SqlitePool) {
    let name = "Sample project";
    let project = project::create(&pool, name.into()).await.unwrap();

    assert_eq!(project.name, name);
}

#[flagrant::test]
async fn create_feature_with_no_value(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = feature::create(&pool, &environment, "sample".into(), None, true)
        .await
        .unwrap();

    assert!(feature.is_enabled);
    assert!(feature.variants.is_empty());
}

#[flagrant::test]
async fn create_feature_with_default_value(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let value = FeatureValue("{\"foo\": 2}".into(), FeatureValueType::Json);
    let feature = feature::create(
        &pool,
        &environment,
        "featuriozzo".into(),
        Some(value.clone()),
        true,
    )
    .await
    .unwrap();

    assert_eq!(feature.variants.len(), 1);
    assert_eq!(feature.get_default_value().unwrap(), value);

    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 100);
    assert!(default_variant.is_control());
}

#[flagrant::test]
async fn create_feature_with_missing_default_variant_in_other_env(pool: SqlitePool) {
    let (project, environment1) = create_context(&pool).await;
    let environment2 = create_environment(&pool, &project).await;

    let feature = create_feature(&pool, &environment1, Some("foo")).await;
    variant::create(&pool, &environment1, &feature, "bar".into(), 40)
        .await
        .unwrap();

    // no default variant in environment2, hence list of variants is empty even though
    // some have been created in environment1.
    let feature = feature::fetch(&pool, &environment2, feature.id)
        .await
        .unwrap();
    assert!(feature.variants.is_empty());

    // after adding default variant, a list consisting of default- and previously created
    // variant should be returned.
    let mut conn = pool.acquire().await.unwrap();
    variant::upsert_default(&mut conn, &environment2, &feature, "bazz".into())
        .await
        .unwrap();

    let feature = feature::fetch(&pool, &environment2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.variants.len(), 2);
}

#[flagrant::test]
async fn create_feature_with_different_values_in_envs(pool: SqlitePool) {
    let (project, environment1) = create_context(&pool).await;
    let environment2 = create_environment(&pool, &project).await;

    let mut conn = pool.acquire().await.unwrap();
    let feature = create_feature(&pool, &environment1, Some("foo")).await;

    variant::upsert_default(&mut conn, &environment2, &feature, "bazz".into())
        .await
        .unwrap();

    let fv1 = FeatureValue("foo".to_string(), FeatureValueType::Text);
    let fv2 = FeatureValue("bazz".to_string(), FeatureValueType::Text);

    let feature = feature::fetch(&pool, &environment1, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), Some(fv1));

    let feature = feature::fetch(&pool, &environment2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature.get_default_value(), Some(fv2));
}

#[flagrant::test]
async fn create_feature_with_invalid_name(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    for name in [" ble", "123", "💕333", "foo-bazz"] {
        let feature = feature::create(
            &pool,
            &environment,
            name.into(),
            Some(FeatureValue("foo".into(), FeatureValueType::Text)),
            false,
        )
        .await;
        assert!(feature.is_err())
    }

    // name too long
    let feature = feature::create(
        &pool,
        &environment,
        format!("F_{}", random_string(1024)),
        Some(FeatureValue("foo".into(), FeatureValueType::Text)),
        false,
    )
    .await;
    assert!(feature.is_err())
}

#[flagrant::test(should_fail = true)]
async fn create_feature_with_non_unique_name(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let name = "should_be_unique";

    feature::create(
        &pool,
        &environment,
        name.into(),
        Some(FeatureValue("foo".into(), FeatureValueType::Text)),
        false,
    )
    .await
    .unwrap();

    feature::create(
        &pool,
        &environment,
        name.into(),
        Some(FeatureValue("foo".into(), FeatureValueType::Text)),
        false,
    )
    .await
    .unwrap();
}

#[flagrant::test]
async fn delete_feature_with_default_value(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("foo")).await;

    assert!(feature::delete(&pool, &environment, &feature).await.is_ok());
    assert!(feature::fetch(&pool, &environment, feature.id)
        .await
        .is_err());
}

#[flagrant::test]
async fn delete_feature_with_variants(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("foo")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 10)
        .await
        .unwrap();

    assert!(feature::delete(&pool, &environment, &feature).await.is_ok());
    assert!(feature::fetch(&pool, &environment, feature.id)
        .await
        .is_err());
}

#[flagrant::test]
async fn create_valid_variant(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("foo")).await;
    let variant = variant::create(&pool, &environment, &feature, "bar".into(), 10).await;

    assert!(variant.is_ok());
}

#[flagrant::test]
async fn create_variant_for_feature_with_no_default_variant(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, None).await;
    let variant = variant::create(&pool, &environment, &feature, "bar".into(), 10).await;

    assert!(variant.is_err());
}

#[flagrant::test]
async fn create_variants_with_valid_weights(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 30)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-3".into(), 40)
        .await
        .unwrap();

    let feature = feature::fetch(&pool, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 20);
}

#[flagrant::test]
async fn create_variants_with_exceeding_weight(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-1".into(), 30)
        .await
        .unwrap();

    let exceeding_variant = variant::create(&pool, &environment, &feature, "bar-3".into(), 90);

    assert!(exceeding_variant.await.is_err());
}

#[flagrant::test]
async fn create_variants_with_different_weights_in_envs(pool: SqlitePool) {
    let (project, environment1) = create_context(&pool).await;
    let environment2 = create_environment(&pool, &project).await;

    let feature = create_feature(&pool, &environment1, Some("foo")).await;
    let variant = variant::create(&pool, &environment1, &feature, "bar".into(), 40)
        .await
        .unwrap();

    let mut conn = pool.acquire().await.unwrap();
    variant::upsert_default(&mut conn, &environment2, &feature, "bazz".into())
        .await
        .unwrap();
    variant::update(&pool, &environment2, &variant, "new-bar".into(), 99)
        .await
        .unwrap();

    let variant_env1 = variant::fetch(&pool, &environment1, variant.id)
        .await
        .unwrap();
    let variant_env2 = variant::fetch(&pool, &environment2, variant.id)
        .await
        .unwrap();

    // variant values are common across all environments
    assert_eq!(variant_env1.value, "new-bar");
    assert_eq!(variant_env2.value, "new-bar");

    // ...but weights are not
    assert_eq!(variant_env1.weight, 40);
    assert_eq!(variant_env2.weight, 99);
}

#[flagrant::test(should_fail = true)]
async fn disallow_default_variant_manual_updates(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("foo")).await;
    let default_variant = feature.get_default_variant().unwrap();

    variant::update(&pool, &environment, default_variant, "bar".into(), 50)
        .await
        .unwrap();
}

#[flagrant::test]
async fn recalculate_default_weight_for_variant_update(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 30)
        .await
        .unwrap();

    let variant = variant::create(&pool, &environment, &feature, "bar-3".into(), 40)
        .await
        .unwrap();
    variant::update(&pool, &environment, &variant, "new-bar-3".into(), 50)
        .await
        .unwrap();

    let feature = feature::fetch(&pool, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 10);
}

#[flagrant::test]
async fn recalculate_default_weight_for_variant_delete(mut pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 30)
        .await
        .unwrap();

    let variant = variant::create(&pool, &environment, &feature, "bar-3".into(), 40)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();

    variant::delete(&mut conn, &environment, &variant)
        .await
        .unwrap();

    let feature = feature::fetch(&pool, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 60);
}

#[flagrant::test]
async fn ignore_default_weight_recalculation_for_exceeding_weight_update(pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 30)
        .await
        .unwrap();

    // update with exceeding weight should fail
    let variant = variant::create(&pool, &environment, &feature, "bar-3".into(), 40)
        .await
        .unwrap();

    assert!(
        variant::update(&pool, &environment, &variant, "new-bar-3".into(), 80)
            .await
            .is_err()
    );

    // default weight should retain old value
    let feature = feature::fetch(&pool, &environment, feature.id)
        .await
        .unwrap();
    let default_variant = feature.get_default_variant().unwrap();

    assert_eq!(default_variant.weight, 20);
}

#[flagrant::test]
async fn disallow_removing_default_variant_when_other_variants_exist(mut pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    variant::create(&pool, &environment, &feature, "bar-1".into(), 10)
        .await
        .unwrap();
    variant::create(&pool, &environment, &feature, "bar-2".into(), 30)
        .await
        .unwrap();

    let variant = feature.get_default_variant().unwrap();
    let mut conn = pool.acquire().await.unwrap();

    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_err());
    assert!(feature.get_default_variant().is_some())
}

#[flagrant::test]
async fn allow_removing_default_variant_when_no_other_variants_exist(mut pool: SqlitePool) {
    let (_, environment) = create_context(&pool).await;
    let feature = create_feature(&pool, &environment, Some("bar")).await;

    let variant = feature.get_default_variant().unwrap();
    let mut conn = pool.acquire().await.unwrap();

    assert!(variant::delete(&mut conn, &environment, variant)
        .await
        .is_ok());
}
