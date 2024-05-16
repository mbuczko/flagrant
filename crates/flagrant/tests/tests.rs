#![allow(dead_code)]

use flagrant::models::{environment, feature, project};
use flagrant_types::{Environment, FeatureValue, FeatureValueType, Project};
use sqlx::SqlitePool;

async fn create_environment(pool: &SqlitePool) -> (Project, Environment) {
    let project = project::create(pool, "fancy project".into()).await.unwrap();

    let environment = environment::create(
        pool,
        &project,
        "production".into(),
        Some("Production environment".into()),
    )
    .await
    .unwrap();

    (project, environment)
}

#[flagrant::test]
async fn create_project(pool: SqlitePool) -> sqlx::Result<()> {
    let name = "Sample project";
    let project = project::create(&pool, name.into()).await.unwrap();

    assert_eq!(project.name, name);
    Ok(())
}

#[flagrant::test]
async fn create_feature(pool: SqlitePool) -> sqlx::Result<()> {
    let (_, environment) = create_environment(&pool).await;
    let feature = feature::create(
        &pool,
        &environment,
        "sample".into(),
        Some(FeatureValue("foo".into(), FeatureValueType::Text)),
        false,
    )
    .await
    .unwrap();

    assert!(!feature.is_enabled);
    Ok(())
}

#[flagrant::test(should_fail = true)]
async fn feature_unique_name(pool: SqlitePool) {
    let (_, environment) = create_environment(&pool).await;
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
