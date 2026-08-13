//! REPL command handlers for feature management.
//!
//! Each public function corresponds to a `FEATURE <op>` or `SET <op>` command,
//! plus the top-level `COMMIT` and `DISCARD` commands:
//!
//! | Command              | Handler                | Description                                           |
//! |----------------------|------------------------|-------------------------------------------------------|
//! | `FEATURE list`       | [`list`]               | List features in the current environment.             |
//! | `FEATURE add`        | [`add`]                | Create a new feature with a default value.            |
//! | `FEATURE use`        | [`r#use`]              | Switch into a feature context.                        |
//! | `FEATURE show`       | [`show`]               | Print details of a feature.                           |
//! | `FEATURE delete`     | [`delete`]             | Delete a feature.                                     |
//! | `FEATURE rename`     | [`rename`]             | Stage a feature name change.                          |
//! | `FEATURE describe`   | [`describe`]           | Stage a feature description.                          |
//! | `FEATURE status`     | [`status`]             | Stage a feature status (`on` / `off` / 'archived').   |
//! | `FEATURE server-side`| [`server-side`]        | Stage a feature server-side only state (`on` / `off`).|
//! | `FEATURE tag`        | [`tag`]                | Stage adding/removing tags on a feature.              |
//! | `UNSET distribution` | [`unset_distribution`] | Clear variant assignments matching a pattern.         |

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
        open_in_editor, segments,
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

/// Splits a `FEATURE use` target into the feature name and an optional identity or segment
/// shortcut: `feature@identity` or `feature+segment`. At most one of the two can be given at
/// a time.
fn split_use_target(name: &str) -> (&str, Option<&str>, Option<&str>) {
    if let Some((feature, identity)) = name.split_once('@') {
        (feature, Some(identity), None)
    } else if let Some((feature, segment)) = name.split_once('+') {
        (feature, None, Some(segment))
    } else {
        (name, None, None)
    }
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
                    is_srv: false,
                    value: parsed,
                },
            )?
        };

        let overrides = fetch_overrides(feature.id, session);
        feature.display(None, &OverridesContext::committed_only(overrides));

        let mut ctx = session.context.write().unwrap();
        ctx.feature = Some(feature);

        index::rebuild(&mut ctx);
        return Ok(());
    }
    bail!("No feature name provided.")
}

/// Stage a feature name change.
///
/// Expected args: `[name]`
///
/// If omitted, opens `$EDITOR` pre-filled with the feature's current (or already-staged)
/// name so it can be edited interactively.
pub fn rename(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let name = match args.get(1) {
        Some(n) => n.to_string(),
        None => {
            let current: &str = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| p.name.as_deref())
                .unwrap_or_else(|| ctx.feature.as_ref().unwrap().name.as_str());

            let edited = open_in_editor(current)?;
            if edited == current {
                println!("No changes made.");
                return Ok(());
            }
            edited
        }
    };

    if name.is_empty() {
        bail!("No name provided.");
    }
    println!("Staged: name = {name}");

    ctx.get_or_init_feature_patch().name = Some(name);
    Ok(())
}

/// Stage a feature description change.
///
/// Expected args: `[description]`
///
/// If omitted, opens `$EDITOR` pre-filled with the feature's current (or already-staged)
/// description so it can be edited interactively; leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let desc = match args.get(1) {
        Some(d) => d.to_string(),
        None => {
            let current: &str = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| p.description.as_deref())
                .unwrap_or_else(|| ctx.feature.as_ref().unwrap().description.as_str());

            let edited = open_in_editor(current)?;

            if edited == current {
                println!("No changes made.");
                return Ok(());
            }
            edited
        }
    };

    println!(
        "Staged: description = {}",
        if desc.is_empty() { "(cleared)" } else { &desc }
    );

    ctx.get_or_init_feature_patch().description = Some(desc);
    Ok(())
}

/// Switch into a feature context by name.
///
/// Expected args: `<feature>`
///
/// Fetches the feature and stores it in the session so that subsequent session-aware
/// commands, like `VARIANT` or `SET` operate on it. Fails if there are uncommitted
/// staged changes.
///
/// The name may carry a shortcut to also switch into an identity or segment context in the
/// same step: `feature@identity` or `feature+segment`.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let (feature_name, identity_str, segment_str) = split_use_target(name);
        if feature_name.is_empty() {
            bail!("No feature name provided.");
        }
        stage::ensure_no_pending(session)?;

        let feature = fetch_feature(feature_name, session)
            .map_err(|_| anyhow::anyhow!("Feature '{}' not found.", feature_name))?;

        let overrides = fetch_overrides(feature.id, session);
        feature.display(None, &OverridesContext::committed_only(overrides));
        {
            let mut ctx = session.context.write().unwrap();
            ctx.feature = Some(feature);
            index::rebuild(&mut ctx);
        }

        if let Some(identity_str) = identity_str {
            identities::switch_to(identity_str, session)?;
        } else if let Some(segment_str) = segment_str {
            segments::switch_to(segment_str, session)?;
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
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let in_context = match args.get(1) {
        Some(name) => ctx
            .feature
            .as_ref()
            .is_some_and(|f| f.name == name.as_ref()),
        None => true,
    };

    if !in_context {
        let name = args.get(1).unwrap();
        drop(ctx);

        let feature = fetch_feature(name, session)?;
        let overrides = fetch_overrides(feature.id, session);

        feature.display(None, &OverridesContext::committed_only(overrides));
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
        if let Some(o) = ipatch
            .overrides
            .iter()
            .find(|o| o.feature_name == feature.name)
        {
            return Some(IdentityPending::Override {
                identity: identity_value,
                variant_value: o.variant_value.clone(),
            });
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

    feature.display(
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
pub fn status(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let (enabled, archived, label) = match args.get(1).map(|arg| arg.to_lowercase()).as_deref() {
        Some("on") => (true, false, "ON"),
        Some("off") => (false, false, "OFF"),
        Some("archived") => (false, true, "ARCHIVED"),
        _ => bail!("Expected one of: on, off, archived"),
    };

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let pending = ctx.get_or_init_feature_patch();
    pending.is_enabled = Some(enabled);
    pending.is_archived = Some(archived);

    println!("Staged: status = {label}");
    Ok(())
}

/// Stage a feature's server-side-only ("srv") flag.
///
/// Expected args: `on`, `off`
pub fn server_side(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let (is_srv, label) = match args.get(1).map(|arg| arg.to_lowercase()).as_deref() {
        Some("on") => (true, "ON"),
        Some("off") => (false, "OFF"),
        _ => bail!("Expected one of: on, off"),
    };

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let pending = ctx.get_or_init_feature_patch();
    pending.is_srv = Some(is_srv);

    println!("Staged: server-side only = {label}");
    Ok(())
}

/// Stage adding or removing one or more tags on the current feature.
///
/// Expected args: `tag1 [tag2 ...]`
///
/// Tags are separated by whitespace. Prefix a tag with `-` to remove it instead of
/// adding it (e.g. `FEATURE tag experimental -ui`).
pub fn tag(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"FEATURE use ...\" to set a context.");
    }

    let ops = parse_tag_ops(&args[1..]);
    if ops.is_empty() {
        bail!("No tags provided.");
    }

    let added = ops
        .iter()
        .filter(|(_, add)| *add)
        .map(|(tag, _)| tag.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let removed = ops
        .iter()
        .filter(|(_, add)| !*add)
        .map(|(tag, _)| tag.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let pending = ctx.get_or_init_feature_patch();
    for (tag, add) in ops {
        stage::stage_tag(pending, tag, add);
    }

    if !added.is_empty() {
        println!("Staged: + tags: {added}");
    }
    if !removed.is_empty() {
        println!("Staged: - tags: {removed}");
    }
    Ok(())
}

/// Parses a list of tag ops out of REPL args - tags may be whitespace-separated across
/// args and/or comma-separated within one arg (e.g. `ui,experiment` and `ui experiment`
/// are equivalent), so a comma typed where a space was meant never ends up embedded in a
/// tag name (which would otherwise fail the tag charset validation as a single bogus tag).
///
/// A `-` prefix marks a removal; when it prefixes a comma-separated group (e.g.
/// `-old,stale`), it applies to every tag in that group, not just the first. Deduplicates
/// by tag name (keeping the first occurrence) and sorts the result by name.
fn parse_tag_ops(args: &[Arg]) -> Vec<(String, bool)> {
    let mut ops: Vec<(String, bool)> = args
        .iter()
        .map(|a| a.trim())
        .filter(|t| !t.is_empty())
        .flat_map(|t| {
            let (add, rest) = match t.strip_prefix('-') {
                Some(rest) => (false, rest),
                None => (true, t),
            };
            rest.split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(move |name| (name.to_string(), add))
                .collect::<Vec<_>>()
        })
        .collect();

    ops.sort_by(|(a, _), (b, _)| a.cmp(b));
    ops.dedup_by(|(a, _), (b, _)| a == b);
    ops
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

/// Stage deletion of a feature by name.
///
/// Expected args: `<name>`
///
/// Switches into the named feature's context first if not already there (same as `FEATURE
/// use`, failing if there are uncommitted staged changes elsewhere), then stages its
/// deletion. Nothing is sent to the API until `COMMIT`; `DISCARD` un-stages it. Once staged,
/// any other pending change for this feature is ignored by the server on commit.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No feature name or value provided.");
    };

    let already_in_context = session
        .context
        .read()
        .unwrap()
        .feature
        .as_ref()
        .is_some_and(|f| f.name == name.as_ref());

    if !already_in_context {
        stage::ensure_no_pending(session)?;

        let feature = fetch_feature(name, session)
            .map_err(|_| anyhow::anyhow!("Feature '{}' not found.", name))?;

        let mut ctx = session.context.write().unwrap();
        ctx.feature = Some(feature);

        index::rebuild(&mut ctx);
    }

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_feature_patch().delete = true;

    println!(
        "Staged: feature '{name}' marked for deletion. Run COMMIT to apply or DISCARD to cancel."
    );
    Ok(())
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
