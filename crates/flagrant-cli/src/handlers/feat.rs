use std::borrow::Cow;

use anyhow::bail;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Feature, FeatureValue, payload::FeatureRequestPayload};

use crate::printer::tabular::Tabular;

/// Adds a new feature - inactive and OFF by default
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|a| Cow::from(a.to_string()))
            .unwrap_or(Cow::Owned(String::default()));
        // .unwrap_or_else(|| Cow::from(multiline_value(editor).unwrap()));

        let parsed = val.parse().unwrap_or_else(|_| FeatureValue::build(&val));
        let feature = ctx.client.post::<_, Feature>(
            res.subpath("/features"),
            FeatureRequestPayload {
                name: name.to_string(),
                description: args.get(3).map(|d| d.to_string()),
                is_enabled: false,
                value: parsed,
            },
        )?;

        feature.describe();
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Switches to the other feature.
pub fn switch(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let mut ctx = session.context.write().unwrap();
        let res = ctx.environment.as_base_resource();
        let response = ctx
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            feature.describe();
            ctx.feature = Some(feature);
        }
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Changes value of given feature.
pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|a| Cow::from(a.to_string()))
            .unwrap_or(Cow::Owned(String::default()));
        //.unwrap_or_else(|| Cow::from(multiline_value(editor).unwrap()));

        let response = ctx
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            let cloned = val
                .parse()
                .unwrap_or_else(|_| feature.get_default_value().clone_with(&val));

            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.value = cloned;
            ctx.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature: Feature = ctx.client.get(res.subpath(&subpath))?;

            feature.describe();
            return Ok(());
        }
        bail!("Feature not found.");
    }
    bail!("No feature name provided.");
}

/// Switches feature on.
pub fn on(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    onoff(args, session, true)
}

/// Switches feature off.
pub fn off(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    onoff(args, session, false)
}

/// Switches feature on/off.
fn onoff(args: &[Arg], session: &Session<Connection>, on: bool) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let response = ctx
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            let subpath = format!("/features/{}", feature.id);
            let mut payload = FeatureRequestPayload::from(feature);

            payload.is_enabled = on;
            ctx.client.put(res.subpath(&subpath), payload)?;

            // re-fetch feature to be sure it's updated
            let feature = ctx.client.get::<Feature>(res.subpath(&subpath))?;

            feature.describe();
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name provided.")
}

/// Lists all features in a project.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.environment.as_base_resource();

    Feature::list(
        ctx.client
            .get::<Vec<Feature>>(res.subpath("/features"))?
            .as_ref(),
    );
    Ok(())
}

/// Deletes existing feature.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let response = ctx
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{name}")));

        if let Ok(feature) = response {
            ctx.client
                .delete(res.subpath(format!("/features/{}", feature.id)))?;

            println!("Feature removed.");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name or value provided.")
}
