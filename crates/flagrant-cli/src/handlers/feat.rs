use std::collections::VecDeque;

use anyhow::bail;
use ascii_table::{Align, AsciiTable};
use flagrant_types::{Feature, FeatureRequestPayload, FeatureValue, FeatureValueType};
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
        let value = match (args.get(2), args.get(3)) {
            (Some(&value_type), Some(value)) => Some(FeatureValue(value.to_string(), value_type.into())),
            (Some(&value_type), _) => Some(multiline_value(value_type.into(), editor)?),
            _ => None
        };
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
    bail!("No feature name provided.")
}

/// Changes value of given feature.
pub fn value(args: &[&str], session: &ReplSession, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<VecDeque<Feature>>(res.subpath(format!("/features?name={name}")));

        if let Ok(mut features) = result && !features.is_empty() {
            let feature = features.pop_front().unwrap();
            let subpath = format!("/features/{}", feature.id);
            let value = match (args.get(2), args.get(3)) {
                (Some(&value_type), Some(value)) => FeatureValue(value.to_string(), value_type.into()),
                (Some(&value_type), _) => multiline_value(value_type.into(), editor)?,
                (_, _) => multiline_value(feature.value_type.clone(), editor)?,
            };
            let mut payload = FeatureRequestPayload::from(feature);

            payload.value = Some(value);
            ssn.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = ssn.client.get(res.subpath(&subpath))?;

            println!("{feature}");
            return Ok(());
        }
        bail!("Feature not found.");
    }
    bail!("No feature name provided.");
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

        if let Ok(mut features) = result
            && !features.is_empty()
        {
            let feature = features.pop_front().unwrap();
            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.is_enabled = on;
            ssn.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature = ssn.client.get::<Feature>(res.subpath(&subpath))?;

            println!("{feature}");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}

/// Lists all features in a project.
pub fn list(_args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    let ssn = session.borrow();
    let res = ssn.environment.as_base_resource();
    let feats: Vec<Feature> = ssn.client.get(res.subpath("/features"))?;

    let mut ascii_table = AsciiTable::default();
    let mut vecs = Vec::with_capacity(feats.len() + 1);

    ascii_table.column(0).set_header("ID");
    ascii_table.column(1).set_header("NAME");
    ascii_table.column(2).set_header("ENABLED?").set_align(Align::Center);
    ascii_table.column(3).set_header("VALUE");

    for mut feat in feats {
        let toggle = if feat.is_enabled { "▣" } else { "▢" };
        let val = match feat.variants.len() {
            0 => String::default(),
            _ => feat.variants.swap_remove(0).value
        };
        vecs.push(vec![
            feat.id.to_string(),
            feat.name.clone(),
            toggle.to_string(),
            val
        ]);
    }
    ascii_table.print(vecs);
    Ok(())
}

/// Deletes existing feature.
pub fn delete(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();
        let result = ssn
            .client
            .get::<VecDeque<Feature>>(res.subpath(format!("/features?name={name}")));

        if let Ok(mut features) = result
            && !features.is_empty()
        {
            let feature = features.pop_front().unwrap();

            ssn.client
                .delete(res.subpath(format!("/features/{}", feature.id)))?;

            println!("Feature removed.");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name or value provided.")
}

fn multiline_value(value_type: FeatureValueType, editor: &mut ReplEditor) -> anyhow::Result<FeatureValue> {
    editor.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::NONE),
        EventHandler::Simple(Cmd::Newline),
    );
    editor.bind_sequence(
        KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::AcceptLine),
    );

    println!("--- Editing '{value_type}' value. Press CTRL-D to finish ---");
    let value = editor.readline("")?;

    // restore default behaviour of Enter and CTRL-D keys
    editor.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::NONE),
        EventHandler::Simple(Cmd::AcceptLine),
    );
    editor.bind_sequence(
        KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::EndOfFile),
    );
    Ok(FeatureValue(value, value_type))
}
