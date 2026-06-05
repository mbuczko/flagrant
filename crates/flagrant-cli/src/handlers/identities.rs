//! REPL command handlers for identity management.
//!
//! | Command                        | Handler         | Description                                         |
//! |--------------------------------|-----------------|-----------------------------------------------------|
//! | `IDENTITY add`                 | [`add`]         | Create or upsert an identity with optional traits.  |
//! | `IDENTITY list`                | [`list`]        | List up to 10 identities, optionally filtered.      |
//! | `IDENTITY describe`            | [`describe`]    | Print details of an identity with its traits.       |
//! | `IDENTITY delete`              | [`delete`]      | Delete an identity by its string value.             |
//! | `IDENTITY use`                 | [`r#use`]       | Switch into an identity context.                    |
//! | `SET trait <name:value>`       | [`set_trait`]   | Stage a trait value change for the current identity.|
//! | `SET identity <value>`         | [`set_identity`]| Stage an identity rename.                           |
//! | `SET override [value]`         | [`set_override`]| Pin the identity to a specific feature variant.     |
//! | `UNSET trait <name>`           | [`unset_trait`] | Stage a trait removal for the current identity.     |
//! | `COMMIT`                       | [`commit`]      | Send staged trait changes to the API.               |
//! | `DISCARD`                      | [`discard`]     | Drop all staged trait changes.                      |

use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureValue, IdentityWithTraits, TraitValue,
    payload::{
        FeaturePatch, IdentityOverridePatch, IdentityPatch, IdentityTraitPayload,
        NewIdentityPayload, OverridePayload,
    },
};

use crate::{
    handlers::{
        edit_in_editor,
        internal::{stage, variants as effective},
    },
    printer::tabular::{DescribeWithVariant, Tabular},
};

/// Print details of an identity with its traits.
///
/// Expected args: `[identity]`
///
/// If an identity argument is provided, fetches and describes that identity.
/// Otherwise describes the identity in the current context.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let identity = resolve_identity(&ctx, identity_str)?;

        describe_identity(&ctx, &identity, None);
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(identity) = &ctx.identity {
            let patch = ctx.identity_patch.as_ref().filter(|p| !p.is_empty());
            describe_identity(&ctx, identity, patch);
        } else {
            bail!("Not in an identity context. Set the context with: \"IDENTITY use\" command.")
        }
    }
    Ok(())
}

/// Fetches the variant currently assigned to `identity_value` for the active feature context.
///
/// Returns `None` if the identity has no assignment yet, `Some(value)` if assigned.
/// Should only be called when already confirmed to be in a feature context.
fn describe_identity(
    ctx: &Connection,
    identity: &IdentityWithTraits,
    patch: Option<&IdentityPatch>,
) {
    if let Some(feature) = ctx.feature.as_ref() {
        let assignment = effective::fetch_variant_assignment(ctx, &identity.value);
        let display = match assignment.as_deref() {
            Some(v) => format!("{} → {}", feature.name.bright_blue(), v),
            None => format!(
                "{} → {}",
                feature.name.bright_blue(),
                "(not yet assigned)".dimmed()
            ),
        };
        identity.describe_with_variant(patch, Some(&display));
    } else {
        let assignments = effective::fetch_all_variant_assignments(ctx, &identity.value);
        let display = if assignments.is_empty() {
            None
        } else {
            Some(
                assignments
                    .iter()
                    .map(|iv| format!("{} → {}", iv.feature_name.bright_blue(), iv.feature_value))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };
        identity.describe_with_variant(patch, display.as_deref());
    }
}

/// Create or upsert an identity with optional traits, then switch into its context.
///
/// Expected args: `<identity> [trait:value ...]`
///
/// Traits are separated by spaces; each in `name:value` form. Values are
/// auto-typed (bool → i32 → f32 → str).
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
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
            let res = ctx.project.as_base_resource();
            ctx.client.post::<_, IdentityWithTraits>(
                res.subpath("/identities"),
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
        identity.describe(None);
        session.context.write().unwrap().identity = Some(identity);
        return Ok(());
    }
    bail!("No identity provided.")
}

/// List identities, optionally filtered by pattern.
///
/// Expected args: `[pattern]`
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project.as_base_resource();
    let pattern = args
        .get(1)
        .map(|a| format!("?pattern={a}"))
        .unwrap_or_default();
    let identities = ctx
        .client
        .get::<Vec<IdentityWithTraits>>(res.subpath(format!("/identities{pattern}")))?;

    IdentityWithTraits::list(&identities);
    Ok(())
}

/// Delete an identity by its exact string value.
///
/// Expected args: `<identity>`
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.project.as_base_resource();
        let identity = resolve_identity(&ctx, identity_str)?;
        ctx.client
            .delete(res.subpath(format!("/identities/{}", identity.value)))?;

        println!("Identity removed.");
        return Ok(());
    }
    bail!("No identity provided.")
}

/// Switch into an identity context.
///
/// Expected args: `<identity>`
///
/// Fetches the identity and stores it in the session so that subsequent `SET`
/// and `UNSET` commands stage trait changes for it. Fails if there are
/// uncommitted staged trait changes.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        {
            let ctx = session.context.read().unwrap();
            if ctx.has_identity_pending() {
                bail!("You have uncommitted trait changes. Run `COMMIT` or `DISCARD` first.");
            }
        }
        let identity = {
            let ctx = session.context.read().unwrap();
            resolve_identity(&ctx, identity_str)?
        };

        {
            let ctx = session.context.read().unwrap();
            describe_identity(&ctx, &identity, None);
        }
        session.context.write().unwrap().identity = Some(identity);
        return Ok(());
    }
    bail!("No identity provided.")
}

/// Stage a trait value change for the current identity.
///
/// Expected args: `trait <name:value>`
///
/// The value is auto-typed (bool → i32 → f32 → str).
pub fn set_trait(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(arg) = args.get(1) {
        if let Some((name, value)) = arg.split_once(':') {
            let mut ctx = session.context.write().unwrap();
            if let Some(identity) = &ctx.identity {
                let trait_exists = identity.traits.iter().any(|t| t.name == name);
                let trait_value = TraitValue::build(value);

                stage::stage_trait(
                    ctx.get_or_init_identity_patch(),
                    trait_exists,
                    name.to_string(),
                    trait_value,
                );
                return Ok(());
            }
            bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
        }
    }
    bail!("Usage: SET trait <name:value>")
}

/// Stage a trait removal for the current identity.
///
/// Expected args: `trait <name>`
pub fn unset_trait(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let mut ctx = session.context.write().unwrap();
        if ctx.identity.is_none() {
            bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
        }
        stage::stage_trait_delete(ctx.get_or_init_identity_patch(), name.to_string());
        return Ok(());
    }
    bail!("Usage: UNSET trait <name>")
}

/// Stage an identity rename.
///
/// Expected args: `identity <value>`
pub fn set_identity(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(value) = args.get(1) {
        let mut ctx = session.context.write().unwrap();
        if ctx.identity.is_none() {
            bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
        }
        stage::stage_identity(ctx.get_or_init_identity_patch(), value.to_string());
        return Ok(());
    }
    bail!("Usage: SET identity <value>")
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
    // Gather everything under a read lock, including opening the editor if needed.
    let (feature_name, identity_value, raw) = {
        let ctx = session.context.read().unwrap();
        let feature = ctx
            .feature
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not in a feature context."))?;
        let identity = ctx
            .identity
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not in an identity context."))?;

        let raw = if let Some(val) = args.get(1) {
            val.to_string()
        } else {
            let variant_path = ctx.env_resource().subpath(format!(
                "/features/{}/identities/{}/variant",
                feature.id, identity.value
            ));
            let current_variant_id = ctx
                .client
                .get::<OverridePayload>(variant_path)
                .ok()
                .map(|p| p.variant_id);

            let content = build_override_editor_content(
                feature,
                ctx.feature_patch.as_ref(),
                current_variant_id,
            );
            let edited = edit_in_editor(&content)?;
            strip_comments(&edited)
        };

        (feature.name.clone(), identity.value.clone(), raw)
    };

    if raw.is_empty() {
        bail!("No value provided.");
    }

    // Validate the value matches a known variant for immediate feedback.
    let variant_value = {
        let ctx = session.context.read().unwrap();
        let feature = ctx.feature.as_ref().unwrap();
        let parsed = raw.parse().unwrap_or_else(|_| FeatureValue::build(&raw));
        let variant = effective::effective_variants(feature, ctx.feature_patch.as_ref())
            .into_iter()
            .find(|v| !v.is_deleted && v.value == parsed)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No variant matches value '{raw}'. Use `VARIANT add` to create a new variant first."
                )
            })?;
        variant.value.to_string()
    };

    // Stage the override — replaces any existing override for this feature.
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

    let identity = ctx.identity.as_ref().unwrap().value.clone();
    let path = ctx
        .env_resource()
        .subpath(format!("/identities/{identity}"));

    match ctx.client.patch::<_, IdentityWithTraits>(path, patch) {
        Ok(updated) => {
            describe_identity(&ctx, &updated, None);
            ctx.identity_patch = None;
            ctx.identity = Some(updated);
        }
        Err(err) => eprintln!("Commit failed: {err}"),
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


fn resolve_identity(
    ctx: &flagrant_client::connection::Connection,
    identity_str: &str,
) -> anyhow::Result<IdentityWithTraits> {
    let res = ctx.project.as_base_resource();
    ctx.client
        .get::<IdentityWithTraits>(res.subpath(format!("/identities/{identity_str}")))
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn build_override_editor_content(
    feature: &Feature,
    patch: Option<&FeaturePatch>,
    current_variant_id: Option<i32>,
) -> String {
    let variants = effective::effective_variants(feature, patch);
    let mut content = String::new();
    let mut idx = 1;

    for e in variants.iter().filter(|e| !e.is_control && !e.is_deleted) {
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
        idx += 1;
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
