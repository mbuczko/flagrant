use flagrant::models::{environment, feature, project};
use flagrant_types::{Environment, Feature, FeatureValue, Project};
use rand::Rng;
use sqlx::{Sqlite, pool::PoolConnection};

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

pub async fn create_environment(
    conn: &mut PoolConnection<Sqlite>,
    project: &Project,
) -> Environment {
    environment::create(
        conn,
        project,
        format!("ENV_{}", random_string(32)),
        Some("Lorem ipsum".to_owned()),
    )
    .await
    .unwrap()
}

pub async fn create_context(conn: &mut PoolConnection<Sqlite>) -> (Project, Environment) {
    let project = project::create(conn, "fancy project".to_owned())
        .await
        .unwrap();
    let environment = create_environment(conn, &project).await;

    (project, environment)
}

#[allow(dead_code)]
pub async fn create_feature(
    conn: &mut PoolConnection<Sqlite>,
    environment: &Environment,
    value: &str,
) -> Feature {
    feature::create(
        conn,
        environment,
        format!("F_{}", random_string(10)),
        FeatureValue::Text(value.to_owned()),
        true,
        true,
    )
    .await
    .unwrap()
}
