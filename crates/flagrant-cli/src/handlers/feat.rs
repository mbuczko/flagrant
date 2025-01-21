use std::borrow::Cow;

use anyhow::bail;
use flagrant_client::session::{Resource, Session};
use flagrant_types::{Feature, FeatureValue, payload::FeatureRequestPayload};

use crate::{
    printer::tabular::Tabular,
    repl::{multiline::multiline_value, readline::ReplEditor},
};

/// Adds a new feature.
pub fn add(args: &[&str], session: &Session, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|&a| Cow::from(a))
            .unwrap_or_else(|| Cow::from(multiline_value(editor).unwrap()));

        let parsed = val.parse().unwrap_or_else(|_| FeatureValue::build(&val));
        let feature =
            session
                .client
                .post::<_, Feature>(res.subpath("/features"), FeatureRequestPayload {
                    name: name.to_string(),
                    description: args.get(3).map(|d| d.to_string()),
                    is_enabled: false,
                    value: parsed,
                })?;

        feature.render();
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Changes value of given feature.
pub fn value(args: &[&str], session: &Session, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|&a| Cow::from(a))
            .unwrap_or_else(|| Cow::from(multiline_value(editor).unwrap()));

        let response = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            let cloned = val
                .parse()
                .unwrap_or_else(|_| feature.get_default_value().clone_with(&val));

            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.value = cloned;
            session.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = session.client.get(res.subpath(&subpath))?;

            feature.render();
            return Ok(());
        }
        bail!("Feature not found.");
    }
    bail!("No feature name provided.");
}

/// Switches feature on.
pub fn on(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    onoff(args, session, true)
}

/// Switches feature off.
pub fn off(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    onoff(args, session, false)
}

/// Switches feature on/off.
fn onoff(args: &[&str], session: &Session, on: bool) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let response = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.is_enabled = on;
            session.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature = session.client.get::<Feature>(res.subpath(&subpath))?;

            feature.render();
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}

/// Lists all features in a project.
pub fn list(_args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    let res = session.environment.as_base_resource();
    let feats: Vec<Feature> = session.client.get(res.subpath("/features"))?;

    let mut rows = Vec::with_capacity(feats.len());
    for feat in feats {
        let toggle = if feat.is_enabled { "▣" } else { "▢" };
        let value = feat.get_default_value();

        rows.push([
            feat.id.to_string(),
            feat.name.clone(),
            toggle.to_string(),
            value.to_string(),
        ]);
    }
    Feature::table().render(rows);
    Ok(())
}

/// Deletes existing feature.
pub fn delete(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let response = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            session
                .client
                .delete(res.subpath(format!("/features/{}", feature.id)))?;

            println!("Feature removed.");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name or value provided.")
}
