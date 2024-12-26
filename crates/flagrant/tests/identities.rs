use flagrant::models::{feature, identity, variant};
use flagrant_types::FeatureValue;
use sqlx::{pool::PoolConnection, Sqlite};

use crate::common::create_context;

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
    let idents_count = identity::get_identities(&mut conn, &environment, &feature)
        .await
        .unwrap()
        .iter()
        .filter(|i| i.migrated_id.is_none())
        .collect::<Vec<_>>()
        .len();

    assert_eq!(idents_count, 10);

    let variant = variant::create(
        &mut conn,
        &environment,
        &feature,
        FeatureValue::build("bazz"),
        50,
    )
    .await
    .unwrap();

    let to_migrate_count = identity::get_identities(&mut conn, &environment, &feature)
        .await
        .unwrap()
        .iter()
        .filter(|i| i.migrated_id == Some(variant.id))
        .collect::<Vec<_>>()
        .len();

    assert_eq!(to_migrate_count, 5);
}
