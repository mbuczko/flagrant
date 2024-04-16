use anyhow::bail;
use flagrant_types::{Feature, FeatureValueType, FeatureRequestPayload};
use itertools::Itertools;

use crate::repl::session::{ReplSession, Resource};

/// Adds a new feature
pub fn add(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        let value = args.get(2).map(|s| s.to_string());
        let value_type = FeatureValueType::from(args.get(3));
        let feature = ssn.client.post::<_, Feature>(
            res.to_path("/features"),
            FeatureRequestPayload {
                name: name.to_string(),
                value: value.map(|v| (v, value_type)),
                description: args.get(3).map(|d| d.to_string()),
                is_enabled: false,
            },
        )?;
        println!("Created new feature ({feature})");
        return Ok(());
    }
    bail!("No feature name or value provided.")
}

/// Lists all features in a project
pub fn list(_args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    let ssn = session.borrow();
    let res = ssn.environment.as_base_resource();
    let feats: Vec<Feature> = ssn.client.get(res.to_path("/features"))?;

    println!("{:-^60}", "");
    println!(
        "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
        "id", "name", "enabled?", "value"
    );
    println!("{:-^60}", "");

    let missing = ("(missing)".into(), FeatureValueType::Text);
    for feat in feats {
        println!(
            "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
            feat.id,
            feat.name,
            feat.is_enabled,
            feat.value.as_ref().unwrap_or(&missing).0
        );
    }
    Ok(())
}

/// Changes value of given feature
pub fn value(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, name, value)) = args.iter().collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<Vec<Feature>>(res.to_path(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.remove(0);
            let value_type = FeatureValueType::from(args.get(3));

            ssn.client.put(
                res.to_path(format!("/features/{}", feature.id)),
                FeatureRequestPayload {
                    name: name.to_string(),
                    value: Some((value.to_string(), value_type)),
                    description: None,
                    is_enabled: feature.is_enabled,
                },
            )?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = ssn
                .client
                .get(res.to_path(format!("/features/{}", feature.id)))?;

            println!("Updated feature ({feature})");
            return Ok(());
        }
        bail!("Feature not found.");
    }
    bail!("No feature name or value provided.");
}

/// Switches feature on
pub fn on(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    onoff(args, session, true)
}

/// Switches feature off
pub fn off(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    onoff(args, session, false)
}

/// Switches feature on/off
fn onoff(args: Vec<&str>, session: &ReplSession, on: bool) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<Vec<Feature>>(res.to_path(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.remove(0);

            ssn.client.put(
                res.to_path(format!("/features/{}", feature.id)),
                FeatureRequestPayload {
                    name: feature.name,
                    value: feature.value,
                    description: None,
                    is_enabled: on,
                },
            )?;

            // re-fetch feature to be sure it's updated
            let feature = ssn
                .client
                .get::<Feature>(res.to_path(format!("/features/{}", feature.id)))?;

            println!("Updated feature ({feature})");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}
