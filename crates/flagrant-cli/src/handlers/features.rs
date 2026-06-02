//! REPL command handlers for feature management.
//!
//! Each public function corresponds to a `FEATURE <op>` or `SET <op>` command,
//! plus the top-level `COMMIT` and `DISCARD` commands:
//!
//! | Command              | Handler       | Description                                          |
//! |----------------------|---------------|------------------------------------------------------|
//! | `FEATURE list`       | [`list`]      | List features in the current environment.            |
//! | `FEATURE add`        | [`add`]       | Create a new feature with a default value.           |
//! | `FEATURE use`        | [`r#use`]     | Switch into a feature context.                       |
//! | `FEATURE describe`   | [`describe`]  | Print details of a feature.                          |
//! | `FEATURE delete`     | [`delete`]    | Delete a feature.                                    |
//! | `SET state`          | [`state`]     | Stage a feature state change (`on` / `off`).         |
//! | `SET status`         | [`status`]    | Stage a feature status change (`active`/`inactive`). |
//! | `SET value`          | [`set_value`] | Stage a default value change.                        |
//! | `COMMIT`             | [`commit`]    | Send all staged changes to the API.                  |
//! | `DISCARD`            | [`discard`]   | Drop all staged changes for the current feature.     |

use std::{collections::BTreeSet, ops::Deref};

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureValue,
    payload::{NewFeaturePayload, OverridePayload, VariantPatchOp},
};

use crate::{
    handlers::{edit_in_editor, internal::stage},
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
            if ctx
                .feature_patch
                .as_ref()
                .map(|p| !p.is_empty())
                .unwrap_or(false)
            {
                bail!("You have uncommitted changes. Run `commit` or `discard` first.");
            }
        }
        let ctx = session.context.read().unwrap();
        let res = ctx.env_resource();
        let val = match args.get(2) {
            Some(a) => a.to_string(),
            None => edit_in_editor("")?,
        };
        let parsed = val.parse().unwrap_or_else(|_| FeatureValue::build(&val));
        let feature = ctx.client.post::<_, Feature>(
            res.subpath("/features"),
            NewFeaturePayload {
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

/// Switch into a feature context by name.
///
/// Expected args: `<feature>`
///
/// Fetches the feature and stores it in the session so that subsequent session-aware
/// commands, like `VARIANT` or `SET` operate on it. Fails if there are uncommitted
/// staged changes.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        {
            let ctx = session.context.read().unwrap();
            if ctx
                .feature_patch
                .as_ref()
                .map(|p| !p.is_empty())
                .unwrap_or(false)
            {
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

/// Print details of a feature.
///
/// Expected args: `[feature]`
///
/// If a feature argument is provided, fetches and describes that feature. Otherwise
/// describes the feature in the current context, overlaying any pending staged changes.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        fetch_feature(name, session)?.describe(None);
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(feature) = &ctx.feature {
            feature.describe(ctx.feature_patch.as_ref().filter(|p| !p.is_empty()));
        } else {
            bail!("Not in a feature context. Set the context with: \"FEATURE use\" command.")
        }
    }
    Ok(())
}

/// Stage a feature state change.
///
/// Expected args: `on` or `off`
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

/// Stage a feature status change.
///
/// Expected args: `active` or `inactive`
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
            let edited = edit_in_editor(bare)?;

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

/// Overrides the variant assigned to the current identity for the current feature,
/// bypassing normal distribution.
///
/// Expected args: `[value]`
///
/// When called without a value argument, opens `$EDITOR` pre-filled with all existing
/// variants (shown as comments with weights) so the user can choose one. All comment
/// lines (starting with `#`) are stripped before the value is used.
///
/// The entered value must match an existing variant exactly. To use an arbitrary value,
/// first create a variant with `VARIANT add`.
pub fn set_override(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let feature = ctx
        .feature
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a feature context."))?;
    let identity = ctx
        .identity
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in an identity context."))?;

    let raw: String = if let Some(val) = args.get(1) {
        val.to_string()
    } else {
        // Resolve which variant this identity is currently assigned to (if any)
        // by querying the override endpoint directly (avoids the is_enabled filter
        // of the public API).
        let override_path = ctx.env_resource().subpath(format!(
            "/features/{}/identities/{}/override",
            feature.id, identity.value
        ));
        let current_variant_id = ctx.client.get::<OverridePayload>(override_path).ok().map(|p| p.variant_id);

        let content = build_override_editor_content(feature, current_variant_id);
        let edited = edit_in_editor(&content)?;
        strip_comments(&edited)
    };

    if raw.is_empty() {
        bail!("No value provided.");
    }

    let parsed = raw.parse().unwrap_or_else(|_| FeatureValue::build(&raw));

    let variant = feature
        .variants
        .iter()
        .find(|v| {
            let (_, bare) = v.value.decompose();
            *bare == raw || v.value == parsed
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No variant matches value '{raw}'. Use `VARIANT add` to create a new variant first."
            )
        })?;

    let path = ctx.env_resource().subpath(format!(
        "/features/{}/identities/{}/override",
        feature.id, identity.value
    ));
    ctx.client.put(path, OverridePayload { variant_id: variant.id })?;
    println!(
        "Override set: '{}' → {} (variant id={})",
        identity.value, parsed, variant.id
    );
    Ok(())
}

fn build_override_editor_content(feature: &Feature, current_variant_id: Option<i32>) -> String {
    let mut content = String::new();
    let non_control: Vec<_> = feature
        .variants
        .iter()
        .filter(|v| !v.is_control())
        .collect();

    for (i, variant) in non_control.iter().enumerate() {
        let (_, bare) = variant.value.decompose();
        let current = if current_variant_id == Some(variant.id) {
            " ← current"
        } else {
            ""
        };
        content.push_str(&format!(
            "# variant {} ({}%){}\n{}\n\n",
            i + 1,
            variant.weight,
            current,
            bare
        ));
    }
    if let Some(control) = feature.variants.iter().find(|v| v.is_control()) {
        let (_, bare) = control.value.decompose();
        let current = if current_variant_id == Some(control.id) {
            " ← current"
        } else {
            ""
        };
        content.push_str(&format!(
            "# default value ({}%){}\n{}",
            control.weight, current, bare
        ));
    }
    content
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
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

    match ctx.client.patch::<_, Feature>(path, patch) {
        Ok(updated) => {
            updated.describe(None);
            ctx.feature_patch = None;
            ctx.feature = Some(updated);
        }
        Err(err) => eprintln!("Commit failed: {err}"),
    }
    Ok(())
}

/// Drop all staged changes for the current feature.
///
/// Must be called without arguments; passing any argument is an error that hints
/// at the more targeted `VARIANT discard <index>` command.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(
            "To discard a single change use `FEATURE discard <name | state | status | tags>` or `VARIANT discard <index>`."
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
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();

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
