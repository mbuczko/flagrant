use std::ops::Deref;

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureOverride, RolloutConfig, RolloutStatus, RolloutStep, VariantValue,
    payload::{NewFeaturePayload, RolloutPatchOp, SegmentPatchOp},
};

use crate::{
    handlers::{
        internal::{concat_values_for_arg, index, stage},
        prompt_line,
    },
    printer::tabular::{
        Tabular,
        feature::{IdentityPending, OverridesContext},
    },
};

pub(crate) fn fetch_feature(name: &str, session: &Session<Connection>) -> anyhow::Result<Feature> {
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
/// `value` is parsed as a typed [`VariantValue`] (e.g. `json::{banner: true}`, `text::hi`);
/// if omitted, prompts for it inline. The feature is created inactive and in a disabled
/// state.
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        stage::ensure_no_pending(session)?;

        let feature = {
            let ctx = session.context.read().unwrap();
            let res = ctx.env_resource();
            let val = match args.get(2) {
                Some(a) => a.to_string(),
                None => match prompt_line("New value", "")? {
                    Some(v) => v,
                    None => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                },
            };

            let parsed = val.parse().unwrap_or_else(|_| VariantValue::build(&val));
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
/// If omitted, prompts inline pre-filled with the feature's current (or already-staged)
/// name so it can be edited in place.
pub fn rename(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
    }

    let name = match args.get(1) {
        Some(n) => n.to_string(),
        None => {
            let current: &str = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| p.name.as_deref())
                .unwrap_or_else(|| ctx.feature.as_ref().unwrap().name.as_str());

            let Some(edited) = prompt_line("New name", current)? else {
                println!("Cancelled.");
                return Ok(());
            };
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
/// If omitted, prompts inline pre-filled with the feature's current (or already-staged)
/// description so it can be edited in place; leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
    }

    let desc = match args.get(1) {
        Some(d) => d.to_string(),
        None => {
            let current: &str = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| p.description.as_deref())
                .unwrap_or_else(|| ctx.feature.as_ref().unwrap().description.as_str());

            let Some(edited) = prompt_line("New description", current)? else {
                println!("Cancelled.");
                return Ok(());
            };

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

/// Switch the session into a feature context by name.
///
/// Fetches the feature and stores it in the session so that subsequent session-aware
/// commands, like `VARIANT` or `SET` operate on it. Fails if there are uncommitted
/// staged changes. Shared entry point used by both the top-level `USE <feature>` and
/// `environments::switch_to` (`USE /environment`, to re-enter the previously active
/// feature after switching environments).
pub(crate) fn switch_to(feature_name: &str, session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;

    let feature = fetch_feature(feature_name, session)
        .map_err(|_| anyhow::anyhow!("Feature '{}' not found.", feature_name))?;

    let overrides = fetch_overrides(feature.id, session);
    feature.display(None, &OverridesContext::committed_only(overrides));

    let mut ctx = session.context.write().unwrap();
    ctx.feature = Some(feature);

    index::rebuild(&mut ctx);
    Ok(())
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
        anyhow::anyhow!(
            "Not in a feature context. Set the context with: \"USE <feature>\" command."
        )
    })?;
    let patch = ctx.feature_patch.as_ref().filter(|p| !p.is_empty());
    let overrides = fetch_overrides(feature.id, session);

    let identity_pending = ctx.identity_patch.as_ref().and_then(|ipatch| {
        let identity_value = ctx.identity.as_ref()?.value.clone();

        // Any recent unset overrides (discarded overrides)?
        if ipatch.unset_overrides.contains(&feature.name) {
            return Some(IdentityPending::UnsetOverride(identity_value));
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
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
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
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
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
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
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
            anyhow::anyhow!("Not within a feature context. Use \"USE <feature>\" to set a context.")
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

/// Manages the current feature's progressive rollout - a single alternative variant's
/// weight ramping up over a defined schedule of steps.
///
/// Expected args:
/// - `rules <w1>:<dur1> [<w2>:<dur2> ...] <100>` - stage a new schedule, e.g.
///   `10:6h 50:2d 80:30m 100`. Each duration accepts an `s`/`m`/`h`/`d` suffix; the last
///   token is the terminal step and must be a bare weight with no duration. Committing
///   this immediately activates the schedule, from step 0, in *every* environment of the
///   project - there's no separate per-environment activation step. Progression itself
///   still gates independently per environment from there on (each has its own
///   minimum-sample-size and hold-duration checks).
/// - `sample <n>` - stage a new minimum-sample-size gate for the currently staged (or
///   already committed) schedule.
/// - `delete` - stage removing the rollout entirely (clears the schedule and every
///   environment's progression, not just this one).
/// - `status` - print the live progression status. Not staged: fetched immediately, since
///   progression itself only ever advances lazily on the server, on the next read.
pub fn progressive(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    match args.get(1).map(|a| a.to_lowercase()).as_deref() {
        Some("rules") => progressive_rules(&args[2..], session),
        Some("sample") => progressive_sample(args.get(2), session),
        Some("delete") => progressive_delete(session),
        Some("status") => progressive_status(session),
        _ => bail!("Expected one of: rules, sample, delete, status"),
    }
}

/// The rollout config that a new `rules`/`sample` op would apply on top of - the staged
/// one if present, otherwise whatever is already committed.
fn effective_rollout(ctx: &Connection) -> Option<RolloutConfig> {
    match ctx.feature_patch.as_ref().and_then(|p| p.rollout.as_ref()) {
        Some(RolloutPatchOp::Set(cfg)) => Some(cfg.clone()),
        Some(RolloutPatchOp::Unset) => None,
        None => ctx.feature.as_ref().and_then(|f| f.rollout.clone()),
    }
}

fn progressive_rules(tokens: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
    }
    if tokens.is_empty() {
        bail!("Usage: FEATURE progressive rules <w1>:<dur1> [<w2>:<dur2> ...] <100>");
    }

    let min_sample_size = effective_rollout(&ctx)
        .map(|c| c.min_sample_size)
        .unwrap_or_else(RolloutConfig::default_min_sample_size);

    let n = tokens.len();
    let mut steps = Vec::with_capacity(n);

    for (i, tok) in tokens.iter().enumerate() {
        let is_last = i + 1 == n;
        let tok_str: &str = tok;

        match tok_str.split_once(':') {
            Some((w, dur)) => {
                if is_last {
                    bail!(
                        "The last step ('{tok_str}') must be a bare weight (e.g. 100), with no duration - it's terminal."
                    );
                }
                let weight: u8 = w
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid weight '{w}' in step '{tok_str}'."))?;
                steps.push(RolloutStep {
                    weight,
                    hold_for_secs: Some(parse_duration(dur)?),
                });
            }
            None => {
                if !is_last {
                    bail!(
                        "Step '{tok_str}' is missing a duration (e.g. {tok_str}:6h) - only the last step is terminal."
                    );
                }
                let weight: u8 = tok_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid weight '{tok_str}'."))?;
                steps.push(RolloutStep {
                    weight,
                    hold_for_secs: None,
                });
            }
        }
    }

    let cfg = RolloutConfig {
        min_sample_size,
        steps,
    };
    cfg.validate_steps().map_err(|e| anyhow::anyhow!(e))?;

    let schedule = cfg
        .steps
        .iter()
        .map(|s| match s.hold_for_secs {
            Some(secs) => format!("{}% for {}", s.weight, format_duration_str(secs)),
            None => format!("{}%", s.weight),
        })
        .collect::<Vec<_>>()
        .join(" -> ");

    ctx.get_or_init_feature_patch().rollout = Some(RolloutPatchOp::Set(cfg));
    println!("Staged: progressive rollout = {schedule}");
    Ok(())
}

fn progressive_sample(arg: Option<&Arg>, session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
    }
    let n: u32 = arg
        .ok_or_else(|| anyhow::anyhow!("Usage: FEATURE progressive sample <n>"))?
        .to_string()
        .parse()
        .map_err(|_| anyhow::anyhow!("Sample size must be a non-negative number."))?;

    let Some(mut cfg) = effective_rollout(&ctx) else {
        bail!(
            "No progressive rollout configured yet. Use \"FEATURE progressive rules ...\" first."
        );
    };
    cfg.min_sample_size = n;

    ctx.get_or_init_feature_patch().rollout = Some(RolloutPatchOp::Set(cfg));
    println!("Staged: progressive rollout minimum sample size = {n}");
    Ok(())
}

fn progressive_delete(session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.feature.is_none() {
        bail!("Not in a feature context. Use \"USE <feature>\" to set a context.");
    }
    ctx.get_or_init_feature_patch().rollout = Some(RolloutPatchOp::Unset);
    println!("Staged: progressive rollout deleted");
    Ok(())
}

fn progressive_status(session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Use \"USE <feature>\" to set a context.")
    })?;
    let has_config = feature.rollout.is_some();
    let environment_name = ctx.environment.name.clone();
    let path = ctx
        .env_resource()
        .subpath(format!("/features/{}/rollout", feature.id));
    let status: Option<RolloutStatus> = ctx.client.get(path)?;
    drop(ctx);

    match status {
        Some(status) => status.display(None, &()),
        // Shouldn't normally happen - committing `rules` activates every environment at
        // once - but can occur for a schedule set before that guarantee existed, or an
        // environment created before this one joined the project. Re-staging the same
        // schedule re-activates everywhere, this environment included.
        None if has_config => println!(
            "A progressive rollout is configured for this feature, but isn't active in '{environment_name}'.\n\
             Run \"FEATURE progressive rules ...\" with the same schedule to (re)activate it everywhere."
        ),
        None => println!(
            "No progressive rollout configured for this feature. Use \"FEATURE progressive rules ...\" to define one."
        ),
    }
    Ok(())
}

/// Parses a duration like `6h`, `2d`, `30m`, `45s` into seconds - a single numeric value
/// followed by one of `s`/`m`/`h`/`d` (seconds/minutes/hours/days).
fn parse_duration(s: &str) -> anyhow::Result<u32> {
    let s = s.trim();
    if s.len() < 2 {
        bail!("Invalid duration '{s}'. Expected e.g. 6h, 2d, 30m, 45s.");
    }
    let (num, unit) = s.split_at(s.len() - 1);
    let n: u32 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid duration '{s}'. Expected e.g. 6h, 2d, 30m, 45s."))?;
    let multiplier = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => bail!("Invalid duration unit in '{s}'. Expected one of: s, m, h, d."),
    };
    Ok(n * multiplier)
}

/// Formats a duration in seconds back into a compact human string, e.g. `1d 2h`. The
/// inverse of [`parse_duration`], used to echo back a just-staged schedule.
fn format_duration_str(mut secs: u32) -> String {
    let days = secs / 86400;
    secs %= 86400;
    let hours = secs / 3600;
    secs %= 3600;
    let minutes = secs / 60;
    secs %= 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs}s"));
    }
    parts.join(" ")
}
