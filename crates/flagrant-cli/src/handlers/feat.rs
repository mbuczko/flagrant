use std::{borrow::Cow, collections::BTreeSet, ops::Deref};

use anyhow::bail;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Feature, FeatureValue, payload::FeatureRequestPayload};

use crate::printer::tabular::Tabular;

fn fetch_feature(name: &str, session: &Session<Connection>) -> anyhow::Result<Feature> {
    let ctx = session.context.read().unwrap();
    let res = ctx.environment.as_base_resource();
    ctx.client
        .get::<Feature>(res.subpath(format!("/features/{name}")))
}

/// Adds a new feature - inactive and OFF by default
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|a| Cow::from(a.to_string()))
            .unwrap_or(Cow::Owned(String::default()));

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

        feature.describe(None);
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Switches to the other feature.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        {
            let ctx = session.context.read().unwrap();
            if ctx.pending.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
                bail!("You have uncommitted changes. Run `commit` or `discard` first.");
            }
        }
        let feature = fetch_feature(name, session)?;

        feature.describe(None);
        session.context.write().unwrap().feature = Some(feature);
        return Ok(());
    }
    bail!("No feature name provided.")
}

pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        fetch_feature(name, session)?.describe(None);
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(feature) = &ctx.feature {
            let patch = ctx.pending.as_ref().filter(|p| !p.is_empty()).cloned();
            feature.describe(patch);
        } else {
            bail!("Not in a feature context. Set the context with: \"FEATURE use\" command.")
        }
    }
    Ok(())
}

/// Switches the feature on/off
pub fn state(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let enabled = args
        .get(1)
        .map(|arg| matches!(arg.to_lowercase().as_str(), "on"));

    if ctx.feature.is_some()
        && let Some(enabled) = enabled
    {
        ctx.get_or_init_pending().is_enabled = Some(enabled);
        println!("Staged: state = {}", if enabled { "on" } else { "off" });
        return Ok(());
    }
    bail!("Not enough arguments provided")
}

/// Toggles the feature status - active/inactive
pub fn status(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let active = args
        .get(1)
        .map(|arg| matches!(arg.to_lowercase().as_str(), "active"));

    if ctx.feature.is_some()
        && let Some(active) = active
    {
        ctx.get_or_init_pending().is_active = Some(active);
        println!(
            "Staged: status = {}",
            if active { "active" } else { "inactive" }
        );
        return Ok(());
    }
    bail!("Not enough arguments provided")
}

/// Stages a new value for the current feature (buffered - use `commit` to apply).
pub fn set_value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if let Some(feature) = &ctx.feature {
        if let Some(raw) = args.get(1) {
            let parsed = raw
                .parse()
                .unwrap_or_else(|_| feature.get_default_value().clone_with(raw));
            let display = parsed.to_string();
            ctx.get_or_init_pending().value = Some(parsed);
            println!("Staged: value = {display}");
            return Ok(());
        }
        bail!("No value provided.")
    }
    bail!("Not within a feature context.")
}

/// Commits all staged changes for the current feature to the API atomically.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    let feature_id = ctx
        .feature
        .as_ref()
        .map(|f| f.id)
        .ok_or_else(|| anyhow::anyhow!("Not within a feature context."))?;

    let patch = match &ctx.pending {
        Some(p) if !p.is_empty() => p.clone(),
        _ => {
            println!("No pending changes to commit.");
            return Ok(());
        }
    };

    let path = ctx
        .environment
        .as_base_resource()
        .subpath(format!("/features/{feature_id}"));

    match ctx.client.patch::<_, Feature>(path, patch) {
        Ok(updated) => {
            updated.describe(None);
            ctx.pending = None;
            ctx.feature = Some(updated);
        }
        Err(err) => eprintln!("Commit failed: {err}"),
    }
    Ok(())
}

/// Discards all staged changes for the current feature.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(
            "To discard a single change use `FEATURE discard <name | state | status | tags>` or `VARIANT discard <index>`."
        );
    }
    let mut ctx = session.context.write().unwrap();
    if ctx.pending.take().is_some() {
        println!("Pending changes discarded.");
    } else {
        println!("No pending changes.");
    }
    Ok(())
}

/// Lists all features in a project.
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.environment.as_base_resource();

    let tags = concat_values_for_arg("tag", args);
    let status = concat_values_for_arg("status", args);
    let state = concat_values_for_arg("state", args);
    let pat = args[1..]
        .iter()
        .find(|a| !a.contains(":"))
        .map(Deref::deref)
        .unwrap_or("");

    Feature::list(
        ctx.client
            .get::<Vec<Feature>>(res.subpath(format!(
                "/features?tags={tags}&status={status}&state={state}&pattern={pat}"
            )))?
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
            .get::<Feature>(res.subpath(format!("/features/{name}")));

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

/// Extracts and concatenates all comma-separated values for a specific argument name.
///
/// Searches through command arguments for entries matching the pattern `arg:value1,value2,...`,
/// collects all unique values using a BTreeSet (which deduplicates and sorts them),
/// and returns them as a single comma-separated string.
///
/// # Arguments
/// * `arg_name` - The argument name to match (e.g., "tag", "status")
/// * `cmd_args` - Slice of command-line arguments in the format "name:value1,value2,..."
///
/// # Returns
/// A comma-separated string of all unique values found for the given argument.
///
/// # Example
/// ```ignore
/// let args = vec!["tag:foo,bar", "tag:baz,foo", "status:active"];
/// let result = concat_values_for_arg("tag", &args);
/// // result == "bar,baz,foo" (deduplicated and sorted)
/// ```
fn concat_values_for_arg(arg_name: &str, cmd_args: &[Arg]) -> String {
    cmd_args
        .iter()
        .fold(BTreeSet::new(), |mut acc, arg| {
            if let Some((arg, tags)) = arg.split_once(":")
                && arg == arg_name
            {
                acc.extend(tags.split(","));
            }
            acc
        })
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}
