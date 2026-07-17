use flagrant::models::{environment, feature, project, segment};
use flagrant_types::{
    Environment, Feature, FeatureValue, GroupConnector, Project, Segment, SegmentDriver,
    Comparator,
    payload::{SegmentPatch, SegmentPatchOp},
};
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
    create_environment_from(conn, project, None).await
}

pub async fn create_environment_from<'a>(
    conn: &mut PoolConnection<Sqlite>,
    project: &Project,
    base_env: impl Into<Option<&'a Environment>>,
) -> Environment {
    environment::create(
        conn,
        project,
        format!("ENV_{}", random_string(32)),
        Some("Lorem ipsum".to_owned()),
        base_env.into().map(|base: &Environment| base.name.clone()),
    )
    .await
    .unwrap()
}

pub async fn create_context(conn: &mut PoolConnection<Sqlite>) -> (Project, Environment) {
    let project = project::create(conn, "fancy_project".to_owned())
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
        Some("sample feature".to_owned()),
        FeatureValue::Text(value.to_owned()),
        true,
    )
    .await
    .unwrap()
}

/// Stages and commits `ops` against `segment` in one patch call.
#[allow(dead_code)]
pub async fn apply(
    conn: &mut PoolConnection<Sqlite>,
    project: &Project,
    segment: Segment,
    ops: Vec<SegmentPatchOp>,
) -> Segment {
    segment::patch(conn, project, segment, SegmentPatch { ops })
        .await
        .unwrap()
}

#[allow(dead_code)]
pub fn add_rule(
    group_label: &str,
    driver: SegmentDriver,
    comparator: Comparator,
    value: &str,
) -> SegmentPatchOp {
    SegmentPatchOp::AddRule {
        group_label: group_label.to_owned(),
        driver,
        comparator,
        value: value.to_owned(),
    }
}

#[allow(dead_code)]
pub fn add_group(connector: Option<GroupConnector>) -> SegmentPatchOp {
    SegmentPatchOp::AddGroup {
        connector,
        description: None,
    }
}
