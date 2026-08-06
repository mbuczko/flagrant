//! REPL command handlers for identity management.
//!
//! | Command                          | Handler           | Description                                         |
//! |----------------------------------|-------------------|-----------------------------------------------------|
//! | `IDENTITY add`                   | [`add`]           | Create or upsert an identity with optional traits.  |
//! | `IDENTITY list`                  | [`list`]          | List up to 10 identities, optionally filtered.      |
//! | `IDENTITY show`                  | [`show`]          | Print details of an identity with its traits.       |
//! | `IDENTITY delete`                | [`delete`]        | Delete identities matching a pattern (`*` wildcard).|
//! | `IDENTITY use`                   | [`r#use`]         | Switch into an identity context.                    |
//! | `IDENTITY trait <name:value...>` | [`r#trait`]       | Stage trait value changes/removals.                 |
//! | `OVERRIDE add [value]`           | [`set_override`]  | Pin the identity to a specific feature variant.     |
//! | `OVERRIDE delete`                | [`unset_override`]| Unpin the identitfy from pinned feature variant.    |
//! | `COMMIT`                         | [`commit`]        | Send staged trait changes to the API.               |
//! | `DISCARD`                        | [`discard`]       | Drop all staged trait changes.                      |

use std::ops::Deref;

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureValue, IdentityVariant, IdentityWithTraits, TraitValue,
    payload::{
        FeaturePatch, IdentityOverridePatch, IdentityTraitPayload, NewIdentityPayload,
        VariantPatchOp,
    },
};

use crate::{
    handlers::{
        features,
        internal::{
            concat_values_for_arg, effectives as effective, extract_single_value, index, stage,
        },
        open_in_editor,
    },
    printer::tabular::Tabular,
};

/// Print details of an identity with its traits.
///
/// Expected args: `[identity]`
///
/// If an identity argument is provided that names a *different* identity than the one in
/// context, fetches and describes that identity with no staged changes overlaid. Otherwise
/// (no argument, or naming the identity already in context) describes the identity in the
/// current context, overlaying any pending staged changes - since pending state only exists
/// for the in-context identity.
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let in_context = match args.get(1) {
        Some(identity_str) => ctx
            .identity
            .as_ref()
            .is_some_and(|i| i.value == identity_str.as_ref()),
        None => true,
    };

    if in_context {
        let identity = ctx.identity.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Not in an identity context. Set the context with: \"IDENTITY use\" command."
            )
        })?;
        let patch = ctx.identity_patch.as_ref().filter(|p| !p.is_empty());

        identity.display(patch, &fetch_variant_assignments(&ctx, identity));
    } else {
        let identity_str = args.get(1).unwrap();
        let identity = resolve_identity(&ctx, identity_str)?;

        identity.display(None, &fetch_variant_assignments(&ctx, &identity));
    }
    Ok(())
}

/// Create or upsert an identity with optional traits, then switch into its context.
///
/// Expected args: `<identity> [trait:value ...]`
///
/// Traits are separated by spaces; each in `name:value` form. Values are
/// auto-typed (bool → i32 → f32 → str).
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        stage::ensure_no_pending(session)?;
        let trait_payloads: Vec<IdentityTraitPayload> = args[2..]
            .iter()
            .filter_map(|arg| {
                let (name, value) = arg.split_once(':')?;
                Some(IdentityTraitPayload {
                    name: name.to_owned(),
                    value: Some(TraitValue::build(value)),
                })
            })
            .collect();

        let identity = {
            let ctx = session.context.read().unwrap();
            ctx.client.post::<_, IdentityWithTraits>(
                ctx.env_resource().subpath("/identities"),
                NewIdentityPayload {
                    identity: identity_str.to_string(),
                    traits: if trait_payloads.is_empty() {
                        None
                    } else {
                        Some(trait_payloads)
                    },
                },
            )?
        };
        identity.display(None, &vec![]);

        let mut ctx = session.context.write().unwrap();
        ctx.identity = Some(identity);
        ctx.segment = None;

        return Ok(());
    }
    bail!("No identity provided.")
}

/// List identities, optionally filtered by pattern and/or trait.
///
/// Expected args: `[pattern] [trait:a] [trait:a=1] [trait:-b] [trait:-b=2] ...`
///
/// `trait:name` restricts results to identities carrying that trait, regardless of value.
/// `trait:name=value` further restricts to identities whose trait value matches - `value`
/// is coerced to whichever of bool/int/float/string it looks like, so `trait:vip=true`
/// matches the trait however it was typed when stored. A leading `-` excludes instead:
/// `trait:-name` drops identities that carry the trait at all, while `trait:-name=value`
/// only drops identities where the trait has that specific value. Conditions may be given
/// as separate `trait:` args or comma-separated within one, e.g. `trait:vip,-churned`.
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();

    let traits = concat_values_for_arg("trait", args);
    let pat = args[1..]
        .iter()
        .find(|a| !a.contains(":"))
        .map(Deref::deref)
        .unwrap_or("");

    let identities = ctx.client.get::<Vec<IdentityWithTraits>>(
        res.subpath(format!("/identities?traits={traits}&pattern={pat}")),
    )?;

    IdentityWithTraits::list(&identities);
    Ok(())
}

/// Immediately delete every identity matching a pattern, within the current
/// project/environment. Not staged - bulk/wildcard deletion doesn't fit a single entity's
/// patch, so this bypasses `COMMIT`/`DISCARD` entirely, same as before.
///
/// Expected args: `<pattern>`
///
/// `pattern` uses `*` as a wildcard (e.g. "user-*", or "*" to delete every identity in the
/// environment). A pattern without `*` deletes only the identity with that exact value.
pub fn drop_matching(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(pattern) = args.get(1) {
        let ctx = session.context.read().unwrap();
        ctx.client.delete(
            ctx.env_resource()
                .subpath(format!("/identities?pattern={pattern}")),
        )?;

        println!("Identities matching '{pattern}' removed.");
        return Ok(());
    }
    bail!("No pattern provided.")
}

/// Stage deletion of a single identity by its exact value.
///
/// Expected args: `<identity>`
///
/// Switches into the named identity's context first if not already there (same as `IDENTITY
/// use`, failing if there are uncommitted staged changes elsewhere), then stages its
/// deletion. Nothing is sent to the API until `COMMIT`; `DISCARD` un-stages it. Once staged,
/// any other pending change for this identity is ignored by the server on commit.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(identity_str) = args.get(1) else {
        bail!("No identity provided.");
    };

    let already_in_context = session
        .context
        .read()
        .unwrap()
        .identity
        .as_ref()
        .is_some_and(|i| i.value == identity_str.as_ref());

    if !already_in_context {
        stage::ensure_no_pending(session)?;

        let ctx = session.context.read().unwrap();
        let identity = resolve_identity(&ctx, identity_str)?;
        drop(ctx);

        let mut ctx = session.context.write().unwrap();
        ctx.identity = Some(identity);
        ctx.segment = None;
    }

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_identity_patch().delete = true;

    println!(
        "Staged: identity '{identity_str}' marked for deletion. Run COMMIT to apply or DISCARD to cancel."
    );
    Ok(())
}

/// Switch into an identity context.
///
/// Expected args: `<identity>`
///
/// Fetches the identity and stores it in the session so that subsequent `IDENTITY
/// trait` and `OVERRIDE` commands stage changes for it. Fails if there are
/// uncommitted staged trait changes.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        return switch_to(identity_str, session);
    }
    bail!("No identity provided.")
}

/// Switch the session into an identity context by name.
///
/// Shared entry point used by both `IDENTITY use` and the `FEATURE use feature@identity`
/// shortcut. Fails if there are uncommitted staged trait changes.
pub(crate) fn switch_to(identity_str: &str, session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;

    let ctx = session.context.read().unwrap();
    let identity = resolve_identity(&ctx, identity_str)?;

    identity.display(None, &fetch_variant_assignments(&ctx, &identity));
    drop(ctx);

    let mut ctx = session.context.write().unwrap();

    ctx.identity = Some(identity);
    ctx.segment = None;
    Ok(())
}

/// Stage adding/changing or removing one or more traits on the current identity.
///
/// Expected args: `name:value [name2:value2 ...] [-name3 ...]`
///
/// Traits are separated by whitespace, each given as `name:value`. Values are
/// auto-typed (bool → i32 → f32 → str). Prefix a name with `-` to remove that trait
/// instead of setting it (e.g. `IDENTITY trait country:pl -org`).
pub fn r#trait(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if args.len() < 2 {
        bail!("Usage: IDENTITY trait <name:value> [-name ...]");
    }

    let mut sets: Vec<(String, TraitValue)> = Vec::new();
    let mut unsets: Vec<String> = Vec::new();

    for arg in &args[1..] {
        match arg.strip_prefix('-') {
            Some(name) => {
                let name = name.split_once(':').map_or(name, |(n, _)| n);
                if name.is_empty() {
                    bail!("Invalid trait syntax: '{arg}'. Expected name:value or -name to unset.");
                }
                unsets.push(name.to_string());
            }
            None => match arg.split_once(':') {
                Some((name, value)) if !name.is_empty() => {
                    sets.push((name.to_string(), TraitValue::build(value)))
                }
                _ => bail!("Invalid trait syntax: '{arg}'. Expected name:value or -name to unset."),
            },
        }
    }

    let mut ctx = session.context.write().unwrap();

    if ctx.identity.is_none() {
        bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
    }

    let existing: Vec<String> = ctx
        .identity
        .as_ref()
        .unwrap()
        .traits
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let patch = ctx.get_or_init_identity_patch();

    for (name, value) in sets {
        let trait_exists = existing.iter().any(|n| n == &name);
        stage::stage_trait(patch, trait_exists, name, value);
    }
    for name in unsets {
        stage::stage_trait_delete(patch, name);
    }
    Ok(())
}

/// Pins the variant to current identity for the current feature, which results in
/// bypassing normal distribution and returning always chosen feature variant.
///
/// Expected args: `[variant-value]`
///
/// When called without a value argument, opens `$EDITOR` pre-filled with all existing
/// variants (shown as comments with weights) so the user can choose one. All comment
/// lines (starting with `#`) are stripped before the value is used.
///
/// The entered value must match an existing variant exactly. To use an arbitrary value,
/// first create a variant with `VARIANT add`.
pub fn set_override(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    // Gather everything under a read lock, including opening the editor if needed.
    let ctx = session.context.read().unwrap();
    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Use \"FEATURE use ...\" to set a context.")
    })?;
    let identity = ctx.identity.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in an identity context. Use \"IDENTITY use ...\" to set a context.")
    })?;

    let raw = match args.get(1) {
        Some(val) => val.to_string(),
        None => {
            let current_variant_id = fetch_variant_assignments(&ctx, identity)
                .into_iter()
                .find(|iv| iv.feature_id == feature.id && iv.identity_id.is_some())
                .and_then(|iv| iv.variant_id);

            let content = build_override_editor_content(
                feature,
                ctx.feature_patch.as_ref(),
                current_variant_id,
            );
            extract_single_value(&open_in_editor(&content)?)?
        }
    };

    if raw.is_empty() {
        bail!("No value provided.");
    }

    let (feature_name, identity_value) = (feature.name.clone(), identity.value.clone());
    drop(ctx);

    let parsed = raw.parse().unwrap_or_else(|_| FeatureValue::build(&raw));

    // Check whether the value matches an existing (or pending) variant.
    let existing_value = {
        let ctx = session.context.read().unwrap();
        let feature = ctx.feature.as_ref().unwrap();
        effective::effective_variants(feature, ctx.feature_patch.as_ref())
            .into_iter()
            .find(|v| !v.is_deleted && v.value == parsed)
            .map(|v| v.value.to_string())
    };

    let variant_value = match existing_value {
        Some(v) => v,
        None => {
            // No matching variant - stage a new one with 0% weight.
            println!(
                "No variant with value '{}' found. Staged new variant with 0% weight (run DISCARD to undo).",
                parsed
            );

            let value_str = parsed.to_string();
            let mut ctx = session.context.write().unwrap();

            ctx.get_or_init_feature_patch()
                .variants
                .push(VariantPatchOp::Add {
                    value: parsed,
                    weight: 0,
                });

            index::rebuild(&mut ctx);
            value_str
        }
    };

    // Stage the pin - replaces any existing override for this feature.
    let mut ctx = session.context.write().unwrap();
    let pending = ctx.get_or_init_identity_patch();

    pending.overrides.retain(|o| o.feature_name != feature_name);
    pending.overrides.push(IdentityOverridePatch {
        feature_name: feature_name.clone(),
        variant_value: variant_value.clone(),
    });
    println!(
        "Staged: override '{}' → {} for feature '{}'",
        identity_value, variant_value, feature_name
    );
    Ok(())
}

/// Stages removal of the current identity's variant assignment for the current feature.
///
/// On `COMMIT` the identity is freed from its pinned (or any explicit) variant assignment
/// and will be re-distributed on the next feature evaluation.
pub fn unset_override(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Use \"FEATURE use ...\" to set a context.")
    })?;
    let identity = ctx.identity.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in an identity context. Use \"IDENTITY use ...\" to set a context.")
    })?;

    let feature_name = feature.name.clone();
    let identity_value = identity.value.clone();
    let pending = ctx.get_or_init_identity_patch();

    // Remove any staged pin for the same feature (unpin supersedes it).
    pending.overrides.retain(|o| o.feature_name != feature_name);

    // Avoid duplicate unpin entries.
    if !pending.unpins.contains(&feature_name) {
        pending.unpins.push(feature_name.clone());
    }
    println!(
        "Staged: unpin '{}' identity from feature '{}' variant",
        identity_value, feature_name
    );
    Ok(())
}

/// Commit staged trait changes for the current identity to the API.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.identity.is_none() {
        return Ok(());
    }

    let patch = match &ctx.identity_patch {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Ok(()),
    };

    let path = ctx.env_resource().subpath(format!(
        "/identities/{}",
        ctx.identity.as_ref().unwrap().value
    ));

    // Overrides/unpins always target the feature currently in context - `ensure_no_pending`
    // forbids switching away from it while either is staged.
    let touched_feature = (!patch.overrides.is_empty() || !patch.unpins.is_empty())
        .then(|| ctx.feature.as_ref().map(|f| f.id))
        .flatten();

    let updated = ctx
        .client
        .patch::<_, Option<IdentityWithTraits>>(path, patch)
        .map_err(|err| anyhow::anyhow!("Identity commit failed: {err}"))?;

    let Some(updated) = updated else {
        println!(
            "Identity '{}' deleted.",
            ctx.identity.as_ref().unwrap().value
        );
        ctx.identity = None;
        ctx.identity_patch = None;

        return Ok(());
    };

    updated.display(None, &fetch_variant_assignments(&ctx, &updated));
    ctx.identity_patch = None;
    ctx.identity = Some(updated);
    drop(ctx);

    // The feature's OVERRIDES section just changed even though the feature itself may have
    // no pending patch of its own (or had one that was already committed and printed before
    // this override existed) - show it again, so the user doesn't have to run `FEATURE
    // show` separately.
    if let Some(feature_id) = touched_feature {
        features::show_by_id(feature_id, session)?;
    }

    Ok(())
}

/// Drop all staged trait changes for the current identity.
pub fn discard(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.has_identity_pending() {
        ctx.discard_identity_pending();
        println!("Pending changes discarded.");
    }
    Ok(())
}

//
// Helpers
//

/// Fetches all variant assignments for `identity_value` across every feature in the active environment.
///
/// Returns an empty vec if the identity has no assignments or the request fails.
fn fetch_variant_assignments(
    ctx: &Connection,
    identity: &IdentityWithTraits,
) -> Vec<IdentityVariant> {
    let path = ctx
        .env_resource()
        .subpath(format!("/identities/{}/variants", identity.value));
    ctx.client
        .get::<Vec<IdentityVariant>>(path)
        .unwrap_or_default()
}

fn resolve_identity(
    ctx: &flagrant_client::connection::Connection,
    identity_str: &str,
) -> anyhow::Result<IdentityWithTraits> {
    ctx.client.get::<IdentityWithTraits>(
        ctx.env_resource()
            .subpath(format!("/identities/{identity_str}")),
    )
}

fn build_override_editor_content(
    feature: &Feature,
    patch: Option<&FeaturePatch>,
    current_variant_id: Option<i32>,
) -> String {
    let variants = effective::effective_variants(feature, patch);
    let mut content = String::new();

    content.push_str(
        "# Leave exactly ONE variant's value uncommented below (or type a brand new value)\n\
         # to pin this identity to it. Comment out or delete the rest.\n\n",
    );

    for (idx, e) in (1..).zip(variants.iter().filter(|e| !e.is_control && !e.is_deleted)) {
        let staged = if e.value_modified || e.is_staged_add {
            " (staged)"
        } else {
            ""
        };
        let current = if e.id.is_some() && e.id == current_variant_id {
            " ← current"
        } else {
            ""
        };
        content.push_str(&format!(
            "# variant {} ({}%){}{}\n{}\n\n",
            idx, e.weight, staged, current, e.value
        ));
    }

    for e in variants.iter().filter(|e| e.is_control && !e.is_deleted) {
        let (_, bare) = e.value.decompose();
        let staged = if e.value_modified { " (staged)" } else { "" };
        let current = if e.id.is_some() && e.id == current_variant_id {
            " ← current"
        } else {
            ""
        };
        content.push_str(&format!(
            "# default value ({}%){}{}\n{}",
            e.weight, staged, current, bare
        ));
    }

    content
}
