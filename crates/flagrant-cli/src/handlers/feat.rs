use std::collections::VecDeque;

use anyhow::bail;
use flagrant_types::{Feature, FeatureRequestPayload, FeatureValue, FeatureValueType};
use itertools::Itertools;
use rustyline::{Cmd, EventHandler, KeyCode, KeyEvent, Modifiers};

use crate::repl::{
    readline::ReplEditor,
    session::{ReplSession, Resource},
};

/// Adds a new feature.
pub fn add(args: &[&str], session: &ReplSession, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        // Having value-type provided, two following cases may happen:
        //
        //  - command line got terminated right after a value type hence no value was given
        //  - value has been provided alongside with value-type
        //
        // First case enables multiline edit which allows to input more complicated text
        // structures (like json or toml).
        //
        // In second case value and value-type are taken just as they have been provided.

        let value = if let Some(value_type) = args.get(2) {
            let value_type = FeatureValueType::from(*value_type);
            match args.get(3) {
                Some(v) => Some(FeatureValue(v.to_string(), value_type)),
                _ => {
                    editor.bind_sequence(
                        KeyEvent(KeyCode::Enter, Modifiers::NONE),
                        EventHandler::Simple(Cmd::Newline),
                    );
                    editor.bind_sequence(
                        KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
                        EventHandler::Simple(Cmd::AcceptLine),
                    );
                    println!("--- press CTRL-D to finish ---");
                    Some(FeatureValue(editor.readline("")?, value_type))
                }
            }
        } else {
            None
        };

        // restore default behaviour of Enter and CTRL-D keys
        editor.bind_sequence(
            KeyEvent(KeyCode::Enter, Modifiers::NONE),
            EventHandler::Simple(Cmd::AcceptLine),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
            EventHandler::Simple(Cmd::EndOfFile),
        );

        let feature = ssn.client.post::<_, Feature>(
            res.subpath("/features"),
            FeatureRequestPayload {
                name: name.to_string(),
                description: args.get(3).map(|d| d.to_string()),
                is_enabled: false,
                value,
            },
        )?;
        println!("{feature}");
        return Ok(());
    }
    bail!("No feature name or value provided.")
}

/// Lists all features in a project.
pub fn list(_args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    let ssn = session.borrow();
    let res = ssn.environment.as_base_resource();
    let feats: Vec<Feature> = ssn.client.get(res.subpath("/features"))?;

    println!(
        "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
        "ID", "NAME", "ENABLED?", "VALUE"
    );
    println!("{:-^60}", "");

    let missing = "(missing)";
    for feat in feats {
        println!(
            "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
            feat.id,
            feat.name,
            feat.is_enabled,
            feat.get_default_variant()
                .map(|v| v.value.as_str())
                .unwrap_or(missing)
        );
    }
    Ok(())
}

/// Changes value of given feature.
pub fn value(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some((_, name, value)) = args.iter().take(3).collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<VecDeque<Feature>>(res.subpath(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.pop_front().unwrap();
            let subpath = format!("/features/{}", feature.id);
            let value = FeatureValue(value.to_string(), feature.value_type.clone());
            let mut payload = FeatureRequestPayload::from(feature);

            payload.value = Some(value);
            ssn.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = ssn
                .client
                .get(res.subpath(&subpath))?;

            println!("{feature}");
            return Ok(());
        }
        bail!("Feature not found.");
    }
    bail!("No feature name or value provided.");
}

/// Switches feature on.
pub fn on(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    onoff(args, session, true)
}

/// Switches feature off.
pub fn off(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    onoff(args, session, false)
}

/// Switches feature on/off.
fn onoff(args: &[&str], session: &ReplSession, on: bool) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<VecDeque<Feature>>(res.subpath(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.pop_front().unwrap();
            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.is_enabled = on;
            ssn.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature = ssn
                .client
                .get::<Feature>(res.subpath(&subpath))?;

            println!("{feature}");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}

/// Deletes existing feature.
pub fn delete(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<VecDeque<Feature>>(res.subpath(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.pop_front().unwrap();

            ssn.client.delete(res.subpath(format!("/features/{}", feature.id)))?;

            println!("Feature removed.");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name or value provided.")
}
