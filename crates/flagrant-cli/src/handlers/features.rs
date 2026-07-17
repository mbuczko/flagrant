//! REPL command handlers for feature management.
//!
//! Each public function corresponds to a `FEATURE <op>` or `SET <op>` command,
//! plus the top-level `COMMIT` and `DISCARD` commands:
//!
//! | Command              | Handler                | Description                                         |
//! |----------------------|------------------------|-----------------------------------------------------|
//! | `FEATURE list`       | [`list`]               | List features in the current environment.           |
//! | `FEATURE add`        | [`add`]                | Create a new feature with a default value.          |
//! | `FEATURE use`        | [`r#use`]              | Switch into a feature context.                      |
//! | `FEATURE describe`   | [`describe`]           | Print details of a feature.                         |
//! | `FEATURE delete`     | [`delete`]             | Delete a feature.                                   |
//! | `SET status`         | [`set_status`]         | Stage a feature status (`on` / `off` / 'archived'). |
//! | `SET description`    | [`set_description`]    | Stage a feature description.                        |
//! | `SET tags`           | [`set_tags`]           | Stage adding tags to a feature.                     |
//! | `UNSET distribution` | [`unset_distribution`] | Clear variant assignments matching a pattern.       |
//! | `UNSET tags`         | [`unset_tags`]         | Stage removing tags from a feature.                 |
//! | `COMMIT`             | [`commit`]             | Send all staged changes to the API.                 |
//! | `DISCARD`            | [`discard`]            | Drop all staged changes for the current feature.    |

use std::ops::Deref;

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureOverride, FeatureValue,
    payload::{NewFeaturePayload, SegmentPatchOp},
};

use crate::{
    handlers::{
        identities,
        internal::{concat_values_for_arg, index, stage},
        open_in_editor,
    },
    printer::tabular::{
        Tabular,
        feature::{IdentityPending, OverridesContext},
    },
};

fn fetch_feature(name: &str, session: &Session<Connection>) -> anyhow::Result<Feature> {
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();
    ctx.client
        .get::<Feature>(res.subpath(format!("/features/{name}")))
}

fn fetch_overrides(feature_id: i32, session: &Session<Connection>) -> Vec<FeatureOverride> {
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();

    ctx.client
        .get::<Vec<FeatureOverride>>(res.subpath(format!("/features/{feature_id}/overrides")))
        .unwrap_or_default()
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
        stage::ensure_no_pending(session)?;

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

        let overrides = fetch_overrides(feature.id, session);
        feature.describe(None, &OverridesContext::committed_only(overrides));

        let mut ctx = session.context.write().unwrap();
        ctx.feature = Some(feature);

        index::rebuild(&mut ctx);
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
        stage::ensure_no_pending(session)?;

        let feature = fetch_feature(feature_name, session)
            .map_err(|_| anyhow::anyhow!("Feature '{}' not found.", feature_name))?;

        let overrides = fetch_overrides(feature.id, session);
        feature.describe(None, &OverridesContext::committed_only(overrides));
        {
            let mut ctx = session.context.write().unwrap();
            ctx.feature = Some(feature);
            index::rebuild(&mut ctx);
        }

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
/// If a feature argument is provided that names a *different* feature than the one in
/// context, fetches and describes that feature with committed overrides only. Otherwise
/// (no argument, or naming the feature already in context) describes the feature in the
/// current context, overlaying any pending staged changes - since pending state (feature
/// patch, identity override, segment override) only exists for the in-context feature.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let in_context = match args.get(1) {
        Some(name) => ctx.feature.as_ref().is_some_and(|f| f.name == name.as_ref()),
        None => true,
    };

    if !in_context {
        let name = args.get(1).unwrap();
        drop(ctx);

        let feature = fetch_feature(name, session)?;
        let overrides = fetch_overrides(feature.id, session);

        feature.describe(None, &OverridesContext::committed_only(overrides));
        return Ok(());
    }

    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Set the context with: \"FEATURE use\" command.")
    })?;
    let patch = ctx.feature_patch.as_ref().filter(|p| !p.is_empty());
    let overrides = fetch_overrides(feature.id, session);

    let identity_pending = ctx.identity_patch.as_ref().and_then(|ipatch| {
        let identity_value = ctx.identity.as_ref()?.value.clone();

        // Any recent unpins (discarded overrides)?
        if ipatch.unpins.contains(&feature.name) {
            return Some(IdentityPending::Unpin(identity_value));
        }

        // ...or newly added overrides?
        if ipatch.overrides.iter().any(|o| o.feature_name == feature.name) {
            return Some(IdentityPending::Override(identity_value));
        }
        None
    });

    let segment_pending = ctx.segment_patch.as_ref().and_then(|spatch| {
        let seg_name = ctx.segment.as_ref()?.name.clone();
        for op in &spatch.ops {
            match op {
                SegmentPatchOp::SetFeatureOverride {
                    feature_id,
                    variant_weights,
                    ..
                } if *feature_id == feature.id => {
                    return Some((seg_name, Some(variant_weights.clone())));
                }
                SegmentPatchOp::UnsetFeatureOverride { feature_id, .. }
                    if *feature_id == feature.id =>
                {
                    return Some((seg_name, None));
                }
                _ => {}
            }
        }
        None
    });

    feature.describe(
        patch,
        &OverridesContext {
            committed: overrides,
            identity_pending,
            segment_pending,
        },
    );
    drop(ctx);

    index::rebuild(&mut session.context.write().unwrap());
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
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }
    ctx.get_or_init_pending().description = Some(desc.clone());
    println!(
        "Staged: description = {}",
        if desc.is_empty() { "(cleared)" } else { &desc }
    );
    Ok(())
}

/// Stage adding one or more tags to the current feature.
///
/// Expected args: `tag1[, tag2, ...]`
///
/// Tags may be separated by commas, whitespace, or both (e.g. `SET tags beta, experimental`).
/// Adds to the feature's existing tag set - use `UNSET tags <tag1, ...>` to remove tags.
pub fn set_tags(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let tags = parse_tags(&args[1..]);
    if tags.is_empty() {
        bail!("No tags provided.");
    }

    let display = tags.join(", ");
    let pending = ctx.get_or_init_pending();

    for tag in tags {
        stage::stage_tag(pending, tag, true);
    }

    println!("Staged: + tags {display}");
    Ok(())
}

/// Stage removing one or more tags from the current feature.
///
/// Expected args: `tag1[, tag2, ...]`
///
/// Tags may be separated by commas, whitespace, or both (e.g. `UNSET tags beta, experimental`).
pub fn unset_tags(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let tags = parse_tags(&args[1..]);
    if tags.is_empty() {
        bail!("No tags provided.");
    }

    let display = tags.join(", ");
    let pending = ctx.get_or_init_pending();

    for tag in tags {
        stage::stage_tag(pending, tag, false);
    }

    println!("Staged: - tags {display}");
    Ok(())
}

/// Parses a list of tag names out of REPL args, splitting on commas and/or whitespace.
/// Deduplicates and sorts the result.
fn parse_tags(args: &[Arg]) -> Vec<String> {
    let mut tags: Vec<String> = args
        .iter()
        .flat_map(|a| a.split(','))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    tags.sort();
    tags.dedup();
    tags
}

/// Commits all staged changes for the current feature to the API atomically.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let feature_id = ctx.feature.as_ref().map(|f| f.id).ok_or_else(|| {
        anyhow::anyhow!("Not within a feature context. Use \"FEATURE use ...\" to set a context.")
    })?;
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

    // If a segment override for this same feature is about to be committed too (as part of
    // the same top-level COMMIT), skip printing here - `segments::describe_by_id` will show
    // the feature afterward with the up-to-date overrides, so we don't print it twice.
    let defer_to_segment_commit = ctx.segment_patch.as_ref().is_some_and(|p| {
        p.ops.iter().any(|op| {
            matches!(op,
                SegmentPatchOp::SetFeatureOverride { feature_id: fid, .. }
                | SegmentPatchOp::UnsetFeatureOverride { feature_id: fid, .. }
                if *fid == feature_id
            )
        })
    });

    // Same reasoning, but for an identity override/unpin about to be committed for this
    // feature right after this - `identities::commit` will show the feature afterward with
    // the up-to-date overrides.
    let defer_to_identity_commit = ctx
        .identity_patch
        .as_ref()
        .is_some_and(|p| !p.overrides.is_empty() || !p.unpins.is_empty());

    if !defer_to_segment_commit && !defer_to_identity_commit {
        let overrides_path = ctx
            .env_resource()
            .subpath(format!("/features/{}/overrides", updated.id));

        let overrides = ctx
            .client
            .get::<Vec<FeatureOverride>>(overrides_path)
            .unwrap_or_default();

        updated.describe(None, &OverridesContext::committed_only(overrides));
    }

    ctx.feature_patch = None;
    ctx.feature = Some(updated);
    index::rebuild(&mut ctx);

    Ok(())
}

/// Re-fetches a feature by id (with its overrides) and prints its `describe()` view.
///
/// Used after a segment or identity commit that touched a feature's overrides: the feature
/// itself has no pending patch of its own, so [`commit`] never runs for it, but its OVERRIDES
/// section just changed and is worth showing. Refreshes the current feature context too,
/// if it still refers to this feature.
pub(crate) fn describe_by_id(feature_id: i32, session: &Session<Connection>) -> anyhow::Result<()> {
    let updated = fetch_feature(&feature_id.to_string(), session)?;
    let overrides = fetch_overrides(updated.id, session);
    updated.describe(None, &OverridesContext::committed_only(overrides));

    let mut ctx = session.context.write().unwrap();
    if ctx.feature.as_ref().is_some_and(|f| f.id == updated.id) {
        ctx.feature = Some(updated);
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
/// Accepts optional filter arguments of the form `tag:a,b` and `status:on|off|archived`,
/// plus a bare pattern string for name matching.
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx: std::sync::RwLockReadGuard<'_, Connection> = session.context.read().unwrap();
    let res = ctx.env_resource();

    let tags = concat_values_for_arg("tag", args);
    let status = concat_values_for_arg("status", args);
    let pat = args[1..]
        .iter()
        .find(|a| !a.contains(":"))
        .map(Deref::deref)
        .unwrap_or("");

    Feature::list(
        ctx.client
            .get::<Vec<Feature>>(res.subpath(format!(
                "/features?tags={tags}&status={status}&pattern={pat}"
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

            let in_context = ctx.feature.as_ref().is_some_and(|f| f.id == feature.id);
            drop(ctx);

            if in_context {
                let mut ctx = session.context.write().unwrap();

                ctx.feature = None;
                ctx.feature_patch = None;
                ctx.variant_index = vec![];
            }
            println!("Feature removed.");
            return Ok(());
        }
        bail!("No such a feature.")
    }
    bail!("No feature name or value provided.")
}

/// Clears the current feature's variant assignments for every identity matching `pattern`,
/// freeing them to be redistributed on the next evaluation.
///
/// Expected args: `<pattern>`
///
/// `pattern` uses `*` as a wildcard (e.g. "user-*", or "*" to clear every identity's
/// assignment for this feature). Unlike `IDENTITY delete <pattern>`, this only removes the
/// variant assignment - the identities themselves (and their traits) are left untouched.
pub fn unset_distribution(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(pattern) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let feature = ctx.feature.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Not within a feature context. Use \"FEATURE use ...\" to set a context."
            )
        })?;

        ctx.client.delete(ctx.env_resource().subpath(format!(
            "/features/{}/distribution?pattern={pattern}",
            feature.id
        )))?;

        println!(
            "Cleared variant assignments matching '{pattern}' for feature '{}'.",
            feature.name
        );
        return Ok(());
    }
    bail!("No pattern provided.")
}
