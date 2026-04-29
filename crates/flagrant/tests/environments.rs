use common::{create_context, create_environment, create_environment_from, create_feature};
use flagrant::models::{feature, variant};
use flagrant_types::FeatureValue;
use sqlx::{Sqlite, pool::PoolConnection};

mod common;

/// When there is exactly one existing environment, a newly created environment
/// should automatically inherit all feature variants from it.
#[sqlx::test]
async fn create_environment_inherits_from_sole_existing_env(mut conn: PoolConnection<Sqlite>) {
    let (project, env1) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &env1, "foo").await;

    variant::create(
        &mut conn,
        &env1,
        &feature,
        FeatureValue::build("bar"),
        30,
    )
    .await
    .unwrap();

    // env1 is the only environment — env2 should auto-clone from it.
    let env2 = create_environment(&mut conn, &project).await;

    let feature_env2 = feature::get_by_id(&mut conn, &env2, feature.id)
        .await
        .unwrap();

    assert_eq!(feature_env2.get_default_value(), &FeatureValue::build("foo"));

    // Control weight should reflect the inherited non-control variant weight (100 - 30 = 70).
    assert_eq!(feature_env2.get_default_variant().weight, 70);
}

/// When an explicit base environment is provided, a newly created environment
/// should inherit all feature variants from that specific base.
#[sqlx::test]
async fn create_environment_inherits_from_provided_base_env(mut conn: PoolConnection<Sqlite>) {
    let (project, env1) = create_context(&mut conn).await;
    let feature = create_feature(&mut conn, &env1, "foo").await;

    variant::create(
        &mut conn,
        &env1,
        &feature,
        FeatureValue::build("bar"),
        40,
    )
    .await
    .unwrap();

    // Create env2 (auto-clones from env1 since it is the sole env).
    let env2 = create_environment(&mut conn, &project).await;

    // Create env3 explicitly based on env1.
    let env3 = create_environment_from(&mut conn, &project, &env1).await;

    let feature_env3 = feature::get_by_id(&mut conn, &env3, feature.id)
        .await
        .unwrap();

    // env3 inherits value and weights from env1, not env2.
    assert_eq!(feature_env3.get_default_value(), &FeatureValue::build("foo"));
    assert_eq!(feature_env3.get_default_variant().weight, 60);

    // env2 was auto-cloned from env1 as well, so they start identical.
    let feature_env2 = feature::get_by_id(&mut conn, &env2, feature.id)
        .await
        .unwrap();
    assert_eq!(feature_env2.get_default_variant().weight, 60);
}
