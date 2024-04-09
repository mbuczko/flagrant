use anyhow::bail;
use flagrant_types::{Feature, NewFeatureRequestPayload};
use itertools::Itertools;

use crate::repl::session::{ReplSession, Resource};

/// Adds a new feature
pub fn add(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, name, value, value_type)) = args.iter().collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let feature = ssn.client.post::<_, Feature>(
            res.to_path("/features"),
            NewFeatureRequestPayload {
                name: name.to_string(),
                value: Some(value.to_string()),
                value_type: (*value_type).into(),
                description: args.get(3).map(|d| d.to_string()),
                is_enabled: false,
            },
        )?;
        println!("Created new feature {feature}");
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

    for feat in feats {
        println!(
            "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
            feat.id, feat.name, feat.is_enabled, feat.value.unwrap_or_else(|| "(missing)".into()));
    }
    Ok(())
}

/// Changes value of given feature
pub fn value(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, name, value, value_type)) = args.iter().collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn.client.get::<Vec<Feature>>(res.to_path(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.remove(0);

            ssn.client.put(
                res.to_path(format!("/features/{}", feature.id)),
                NewFeatureRequestPayload {
                    name: name.to_string(),
                    value: Some(value.to_string()),
                    value_type: (*value_type).into(),
                    description: None,
                    is_enabled: feature.is_enabled,
                },
            )?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = ssn.client.get(res.to_path(format!("/features/{}", feature.id)))?;

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
        let result = ssn.client.get::<Vec<Feature>>(res.to_path(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.remove(0);

            ssn.client
                .put(res.to_path(format!("/features/{}", feature.id)),
                     NewFeatureRequestPayload {
                         name: feature.name,
                         value: feature.value,
                         value_type: feature.value_type,
                         description: None,
                         is_enabled: on,
                     },
                )?;

            // re-fetch feature to be sure it's updated
            let feature = ssn.client.get::<Feature>(res.to_path(format!("/features/{}", feature.id)))?;

            println!("Updated feature ({feature})");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}
