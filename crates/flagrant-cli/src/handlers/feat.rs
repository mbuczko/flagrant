use anyhow::bail;
use flagrant_types::{Feature, NewFeatureRequestPayload};
use itertools::Itertools;

use crate::repl::context::ReplContext;

/// Adds a new feature
pub fn add(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if let Some((name, value)) = args.iter().collect_tuple() {
        let payload = NewFeatureRequestPayload {
            name: name.to_string(),
            value: value.to_string(),
            description: args.get(3).map(|d| d.to_string()),
            is_enabled: false,
        };
        let feat: Feature = context.read().unwrap().client.post("/features", payload)?;

        return Ok(println!(
            "Created new feature '{}' (id={}, value={})",
            feat.name, feat.id, feat.value
        ));
    }
    bail!("No feature name or value provided.")
}

/// Lists all features in a project
pub fn ls(_args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    let feats: Vec<Feature> = context.read().unwrap().client.get("/features")?;

    println!("{:-^60}", "");
    println!(
        "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
        "id", "name", "enabled?", "value"
    );
    println!("{:-^60}", "");

    for feat in feats {
        println!(
            "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
            feat.id, feat.name, feat.is_enabled, feat.value
        );
    }
    Ok(())
}

/// Changes value of given feature
pub fn val(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if let Some((name, value)) = args.iter().collect_tuple() {
        let client = &context.read().unwrap().client;
        if let Ok(feature) = client.get::<_, Feature>(format!("/features/{name}")) {
            let payload = NewFeatureRequestPayload {
                name: name.to_string(),
                value: value.to_string(),
                description: None,
                is_enabled: feature.is_enabled,
            };
            client.put(format!("/features/{name}"), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = client.get(format!("/features/{name}"))?;

            return Ok(println!(
                "Updated feature (id={}, name={}, value={}, is_enabled={})",
                feature.id, feature.name, feature.value, feature.is_enabled
            ));
        }
        bail!("Feature not found.");
    }
    bail!("No feature name or value provided.");
}

/// Switches feature on/off
pub fn onoff(args: Vec<&str>, context: &ReplContext, on: bool) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let client = &context.read().unwrap().client;
        if let Ok(feature) = client.get::<_, Feature>(format!("/features/{name}")) {
            let payload = NewFeatureRequestPayload {
                name: feature.name,
                value: feature.value,
                description: None,
                is_enabled: on,
            };
            if client.put(format!("/features/{name}"), payload).is_ok() {
                // re-fetch feature to be sure it's updated
                let feature: Feature = client.get(format!("/features/{name}"))?;

                return Ok(println!(
                    "Updated feature (id={}, name={}, value={}, is_enabled={})",
                    feature.id, feature.name, feature.value, feature.is_enabled
                ));
            }
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}

/// Switches feature on
pub fn on(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    onoff(args, context, true)
}

/// Switches feature off
pub fn off(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    onoff(args, context, false)
}
