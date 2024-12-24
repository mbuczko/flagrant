use common::create_context;
use flagrant::models::{feature, identity, variant};
use flagrant_types::FeatureValue;
use sqlx::{pool::PoolConnection, Sqlite};

mod common;

#[sqlx::test]
async fn migrate_identities(mut conn: PoolConnection<Sqlite>) {
    let (_, environment) = create_context(&mut conn).await;
    // let environment2 = create_environment(&mut conn, &project).await;
    let feature = feature::create(
        &mut conn,
        &environment,
        "featuriozzo".to_owned(),
        FeatureValue::build("foo"),
        true,
    )
    .await
    .unwrap();

    for n in 1..=10 {
        identity::get_variants(&mut conn, &environment, format!("identity_{n}"))
            .await
            .unwrap();
    }
    let idents = identity::get_identities(&mut conn, &environment, &feature)
        .await
        .unwrap()
        .iter()
        .map(|i| i.migrated_id.is_none())
        .collect::<Vec<_>>();
    assert_eq!(idents.len(), 10);

    variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bazz"),
        10,
    )
    .await
    .unwrap();

    let idents = identity::get_identities(&mut conn, &environment, &feature)
        .await
        .unwrap()
        .iter()
        .map(|i| i.migrated_id.is_none())
        .collect::<Vec<_>>();

    assert_eq!(idents.len(), 10);
}
