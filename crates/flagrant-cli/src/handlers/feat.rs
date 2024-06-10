use anyhow::bail;
use ascii_table::{Align, AsciiTable};
use flagrant_client::session::{Resource, Session};
use flagrant_types::{payloads::FeatureRequestPayload, tabular::Tabular, Feature, FeatureValue};

use crate::repl::{multiline::multiline_value, readline::ReplEditor};

/// Adds a new feature.
pub fn add(args: &[&str], session: &Session, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|v| v.to_string())
            .unwrap_or_else(|| multiline_value(editor).unwrap());

        let parsed = val.parse().unwrap_or_else(|_| FeatureValue::build(&val));
        let feature = session.client.post::<_, Feature>(
            res.subpath("/features"),
            FeatureRequestPayload {
                name: name.to_string(),
                description: args.get(3).map(|d| d.to_string()),
                is_enabled: false,
                value: Some(parsed),
            },
        )?;

        feature.tabular_print();
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
            .map(|v| v.to_string())
            .unwrap_or_else(|| multiline_value(editor).unwrap());

        if let Ok(feature) = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")))
        {
            let cloned = val.parse().unwrap_or_else(|_| {
                feature
                    .get_default_value()
                    .map(|v| v.clone_with(&val))
                    .unwrap_or_else(|| FeatureValue::Text(val))
            });

            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.value = Some(cloned);
            session.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = session.client.get(res.subpath(&subpath))?;

            feature.tabular_print();
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
        let result = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = result {
            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.is_enabled = on;
            session.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature = session.client.get::<Feature>(res.subpath(&subpath))?;

            feature.tabular_print();
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

    let mut table = AsciiTable::default();
    let mut vecs = Vec::with_capacity(feats.len() + 1);

    table.column(0).set_header("ID");
    table.column(1).set_header("NAME");
    table
        .column(2)
        .set_header("ENABLED?")
        .set_align(Align::Center);
    table.column(3).set_header("VALUE");

    for feat in feats {
        let toggle = if feat.is_enabled { "▣" } else { "▢" };
        let value = feat.get_default_value();

        vecs.push(vec![
            feat.id.to_string(),
            feat.name.clone(),
            toggle.to_string(),
            value.map(|v| v.to_string()).unwrap_or_default(),
        ]);
    }
    table.print(vecs);
    Ok(())
}

/// Deletes existing feature.
pub fn delete(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let result = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = result {
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
