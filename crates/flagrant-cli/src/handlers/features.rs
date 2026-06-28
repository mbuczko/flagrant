//! REPL command handlers for feature management.
//!
//! Each public function corresponds to a `FEATURE <op>` or `SET <op>` command,
//! plus the top-level `COMMIT` and `DISCARD` commands:
//!
//! | Command              | Handler          | Description                                      |
//! |----------------------|------------------|--------------------------------------------------|
//! | `FEATURE list`       | [`list`]         | List features in the current environment.        |
//! | `FEATURE add`        | [`add`]          | Create a new feature with a default value.       |
//! | `FEATURE use`        | [`r#use`]        | Switch into a feature context.                   |
//! | `FEATURE describe`   | [`describe`]     | Print details of a feature.                      |
//! | `FEATURE delete`     | [`delete`]       | Delete a feature.                                |
//! | `SET state`          | [`state`]        | Stage a feature state change (`on` / `off`).     |
//! | `SET archived`       | [`set_archived`] | Stage a feature archivisation (`yes` / `no`).    |
//! | `SET value`          | [`set_value`]    | Stage a default value change.                    |
//! | `COMMIT`             | [`commit`]       | Send all staged changes to the API.              |
//! | `DISCARD`            | [`discard`]      | Drop all staged changes for the current feature. |

use std::{collections::BTreeSet, ops::Deref};

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureValue,
    payload::{NewFeaturePayload, VariantPatchOp},
};

use crate::{
    handlers::{identities, internal::stage, open_in_editor},
    printer::tabular::Tabular,
};
use flagrant_client::connection::VariantRef;

fn fetch_feature(name: &str, session: &Session<Connection>) -> anyhow::Result<Feature> {
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();
    ctx.client
        .get::<Feature>(res.subpath(format!("/features/{name}")))
}

/// Create a new feature in the current environment.
///
/// Expected args: `<feature> [value] [description]`
///
/// `value` is parsed as a typed [`FeatureValue`] (e.g. `json::{banner: true}`, `text::hi`);
/// if omitted, an editor is opened to enter the value interactively. The feature is
/// created inactive and in a disabled state.
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        {
            let ctx = session.context.read().unwrap();
            if ctx.has_feature_pending() {
                bail!("You have uncommitted changes. Run `COMMIT` or `DISCARD` first.");
            }
        }
        let feature = {
            let ctx = session.context.read().unwrap();
            let res = ctx.env_resource();
            let val = match args.get(2) {
                Some(a) => a.to_string(),
                None => open_in_editor("")?,
            };
            let parsed = val.parse().unwrap_or_else(|_| FeatureValue::build(&val));
            ctx.client.post::<_, Feature>(
                res.subpath("/features"),
                NewFeaturePayload {
                    name: name.to_string(),
                    description: args.get(3).map(|d| d.to_string()),
                    is_enabled: false,
                    value: parsed,
                },
            )?
        };

        feature.describe(None, &());
        session.context.write().unwrap().feature = Some(feature);
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Switch into a feature context by name.
///
/// Expected args: `<feature>`
///
/// Fetches the feature and stores it in the session so that subsequent session-aware
/// commands, like `VARIANT` or `SET` operate on it. Fails if there are uncommitted
/// staged changes.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let (feature_name, identity_str) = match name.split_once('@') {
            Some((f, i)) => (f, Some(i)),
            None => (name.deref(), None),
        };
        if feature_name.is_empty() {
            bail!("No feature name provided.");
        }
        {
            let ctx = session.context.read().unwrap();
            if ctx.has_feature_pending() {
                bail!("You have uncommitted changes. Run `COMMIT` or `DISCARD` first.");
            }
        }
        let feature = fetch_feature(feature_name, session)
            .map_err(|_| anyhow::anyhow!("Feature '{}' not found.", feature_name))?;
        feature.describe(None, &());
        session.context.write().unwrap().feature = Some(feature);

        if let Some(identity_str) = identity_str {
            identities::switch_to(identity_str, session)?;
        }
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Print details of a feature.
///
/// Expected args: `[feature]`
///
/// If a feature argument is provided, fetches and describes that feature. Otherwise
/// describes the feature in the current context, overlaying any pending staged changes.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        fetch_feature(name, session)?.describe(None, &());
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(feature) = &ctx.feature {
            feature.describe(ctx.feature_patch.as_ref().filter(|p| !p.is_empty()), &());
        } else {
            bail!("Not in a feature context. Set the context with: \"FEATURE use\" command.")
        }
    }
    Ok(())
}

/// Stage a feature state change.
///
/// Expected args: `on`, `off` and `archived`
pub fn set_status(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let enabled = args
        .get(1)
        .map(|arg| matches!(arg.to_lowercase().as_str(), "on"));
    let archived = args
        .get(1)
        .map(|arg| matches!(arg.to_lowercase().as_str(), "archived"));

    if ctx.feature.is_some() {
        if archived.unwrap_or_default() {
            ctx.get_or_init_pending().is_archived = Some(true);
            ctx.get_or_init_pending().is_enabled = Some(false);
            println!("Staged: status = ARCHIVED");
        } else if let Some(enabled) = enabled {
            ctx.get_or_init_pending().is_enabled = Some(enabled);
            ctx.get_or_init_pending().is_archived = Some(false);
            println!("Staged: status = {}", if enabled { "ON" } else { "OFF" });
        }
        return Ok(());
    }
    bail!("Not enough arguments provided")
}

/// Stage a feature description change.
///
/// Expected args: `[description]` (omit to clear)
pub fn set_description(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let desc = args.get(1).map(|a| a.to_string()).unwrap_or_default();
    let mut ctx = session.context.write().unwrap();
    if ctx.feature.is_none() {
        bail!("Not in a feature context.");
    }
    ctx.get_or_init_pending().description = Some(desc.clone());
    println!(
        "Staged: description = {}",
        if desc.is_empty() { "(cleared)" } else { &desc }
    );
    Ok(())
}

/// Stages a new value for the current feature (i.e. its control variant).
///
/// Expected args: `[value]`
///
/// When called without a value argument, opens `$EDITOR` (falling back to `vi`) pre-filled
/// with the current value so the user can edit it interactively.
pub fn set_value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if let Some(feature) = &ctx.feature {
        let control_id = feature.get_default_variant().id;
        let control_ref = VariantRef::Committed(control_id);

        let raw: String = if let Some(raw) = args.get(1) {
            raw.to_string()
        } else {
            // No value provided — open editor with current bare value (without type prefix).
            // Prefer any already-staged value over the committed one.
            let current_fv = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| {
                    p.variants.iter().find_map(|op| match op {
                        VariantPatchOp::SetValue { id, value } if *id == control_id => Some(value),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| feature.get_default_value());
            let (_, bare) = current_fv.decompose();
            let edited = open_in_editor(bare)?;

            // Type is inferred from the edited content, not the original.
            let parsed = FeatureValue::build(&edited);

            stage::stage_value(ctx.get_or_init_pending(), &control_ref, parsed)?;
            return Ok(());
        };

        let parsed = raw
            .parse()
            .unwrap_or_else(|_| feature.get_default_value().clone_with(&raw));
        let display = parsed.to_string();
        stage::stage_value(ctx.get_or_init_pending(), &control_ref, parsed)?;

        println!("Staged: value = {display}");
        return Ok(());
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

    let patch = match &ctx.feature_patch {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Ok(()),
    };

    let path = ctx
        .env_resource()
        .subpath(format!("/features/{feature_id}"));

    let updated = ctx
        .client
        .patch::<_, Feature>(path, patch)
        .map_err(|err| anyhow::anyhow!("Feature commit failed: {err}"))?;

    updated.describe(None, &());
    ctx.feature_patch = None;
    ctx.feature = Some(updated);
    Ok(())
}

/// Drop all staged changes for the current feature.
///
/// Must be called without arguments; passing any argument is an error that hints
/// at the more targeted `VARIANT discard <index>` command.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(
            "No arguments expected. To discard a single change on variant use `VARIANT discard <index>`."
        );
    }
    let mut ctx = session.context.write().unwrap();
    if ctx.feature_patch.take().is_some() {
        println!("Pending changes discarded.");
    }
    Ok(())
}

/// List features in the current environment.
///
/// Accepts optional filter arguments of the form `tag:a,b`, `status:active`,
/// and `state:on`, plus a bare pattern string for name matching.
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx: std::sync::RwLockReadGuard<'_, Connection> = session.context.read().unwrap();
    let res = ctx.env_resource();

    let tags = concat_values_for_arg("tag", args);
    let archived: String = concat_values_for_arg("archived", args);
    let enabled = concat_values_for_arg("enabled", args);
    let pat = args[1..]
        .iter()
        .find(|a| !a.contains(":"))
        .map(Deref::deref)
        .unwrap_or("");

    Feature::list(
        ctx.client
            .get::<Vec<Feature>>(res.subpath(format!(
                "/features?tags={tags}&archived={archived}&enabled={enabled}&pattern={pat}"
            )))?
            .as_ref(),
    );
    Ok(())
}

/// Delete a feature by name.
///
/// Looks up the feature by name to obtain its id, then issues a DELETE request.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.env_resource();
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
