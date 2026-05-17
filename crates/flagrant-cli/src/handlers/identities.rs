//! REPL command handlers for identity management.
//!
//! | Command                        | Handler         | Description                                         |
//! |--------------------------------|-----------------|-----------------------------------------------------|
//! | `IDENTITY add`                 | [`add`]         | Create or upsert an identity with optional traits.  |
//! | `IDENTITY list`                | [`list`]        | List up to 10 identities, optionally filtered.      |
//! | `IDENTITY delete`              | [`delete`]      | Delete an identity by its string value.             |
//! | `IDENTITY use`                 | [`r#use`]       | Switch into an identity context.                    |
//! | `SET <trait> <value>`          | [`set_trait`]   | Stage a trait value change for the current identity.|
//! | `UNSET <trait>`                | [`unset_trait`] | Stage a trait removal for the current identity.     |
//! | `COMMIT`                       | [`commit`]      | Send staged trait changes to the API.               |
//! | `DISCARD`                      | [`discard`]     | Drop all staged trait changes.                      |

use anyhow::bail;
use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    IdentityWithTraits, TraitValue,
    payload::{IdentityRequestPayload, IdentityTraitPayload},
};

fn describe(identity: &IdentityWithTraits) {
    let title = format!("{} (ID={})", identity.value, identity.id);
    let traits = if identity.traits.is_empty() {
        "(none)".dimmed().to_string()
    } else {
        identity
            .traits
            .iter()
            .map(|t| {
                let val = t
                    .value
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(unset)".dimmed().to_string());
                format!("{} = {}", t.name.bright_blue(), val)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let table = FancyTable::create(FancyTableOpts::default())
        .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
        .add_column(
            None,
            Layout::Expandable(120),
            Align::Left,
            Overflow::Truncate,
            1,
        )
        .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
        .build();

    table.render(vec![&["TRAITS", &traits]]);
}

fn list_all(identities: &[IdentityWithTraits]) {
    let rows: Vec<_> = identities
        .iter()
        .map(|id| {
            let traits = id
                .traits
                .iter()
                .map(|t| {
                    let val = t.value.as_ref().map(|v| v.to_string()).unwrap_or_default();
                    format!("{}:{}", t.name, val)
                })
                .collect::<Vec<_>>()
                .join(", ");
            [id.value.clone(), traits]
        })
        .collect();

    FancyTable::create(FancyTableOpts::default())
        .add_column_named_with_align("IDENTITY".into(), Layout::Fixed(40), Align::Left)
        .add_column_named_with_align("TRAITS".into(), Layout::Expandable(60), Align::Left)
        .width(100)
        .build()
        .render(rows);
}

fn resolve_identity(
    ctx: &flagrant_client::connection::Connection,
    identity_str: &str,
) -> anyhow::Result<IdentityWithTraits> {
    let identities = ctx
        .client
        .get::<Vec<IdentityWithTraits>>(format!("/identities?pattern={identity_str}"))?;
    identities
        .into_iter()
        .find(|i| i.value == identity_str)
        .ok_or_else(|| anyhow::anyhow!("Identity not found: {identity_str}"))
}

/// Create or upsert an identity with optional traits.
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

        let ctx = session.context.read().unwrap();
        let identity = ctx.client.post::<_, IdentityWithTraits>(
            "/identities".to_owned(),
            IdentityRequestPayload {
                identity: identity_str.to_string(),
                traits: if trait_payloads.is_empty() {
                    None
                } else {
                    Some(trait_payloads)
                },
            },
        )?;
        describe(&identity);
        return Ok(());
    }
    bail!("No identity provided.")
}

/// List identities, optionally filtered by pattern.
///
/// Expected args: `[pattern]`
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let pattern = args
        .get(1)
        .map(|a| format!("?pattern={a}"))
        .unwrap_or_default();
    let identities = ctx
        .client
        .get::<Vec<IdentityWithTraits>>(format!("/identities{pattern}"))?;
    list_all(&identities);
    Ok(())
}

/// Delete an identity by its exact string value.
///
/// Expected args: `<identity>`
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(identity_str) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let identity = resolve_identity(&ctx, identity_str)?;
        ctx.client.delete(format!("/identities/{}", identity.id))?;
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
        describe(&identity);
        session.context.write().unwrap().identity = Some(identity);
        return Ok(());
    }
    bail!("No identity provided.")
}

/// Stage a trait value for the current identity.
///
/// Expected args: `<trait> <value>`
///
/// The value is auto-typed (bool → i32 → f32 → str) and stored in encoded
/// form (`type::value`).
pub fn set_trait(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let (Some(name), Some(value)) = (args.get(1), args.get(2)) {
        let mut ctx = session.context.write().unwrap();
        if ctx.identity.is_none() {
            bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
        }
        let trait_value = TraitValue::build(value);
        ctx.pending_traits
            .insert(name.to_string(), Some(trait_value.clone()));
        println!("Staged: {} = {}", name, trait_value);
        return Ok(());
    }
    bail!("Usage: SET <trait> <value>")
}

/// Stage a trait removal for the current identity.
///
/// Expected args: `<trait>`
pub fn unset_trait(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let mut ctx = session.context.write().unwrap();
        if ctx.identity.is_none() {
            bail!("Not in an identity context. Use `IDENTITY use <identity>` first.");
        }
        ctx.pending_traits.insert(name.to_string(), None);
        println!("Staged: unset {name}");
        return Ok(());
    }
    bail!("Usage: UNSET <trait>")
}

/// Commit staged trait changes for the current identity to the API.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.identity.is_none() {
        bail!("Not in an identity context.");
    }
    if ctx.pending_traits.is_empty() {
        println!("No pending changes to commit.");
        return Ok(());
    }

    let identity_id = ctx.identity.as_ref().unwrap().id;

    // Merge staged changes onto current trait set
    let mut merged: std::collections::BTreeMap<String, Option<TraitValue>> = ctx
        .identity
        .as_ref()
        .unwrap()
        .traits
        .iter()
        .map(|t| (t.name.clone(), t.value.clone()))
        .collect();

    for (name, val) in &ctx.pending_traits {
        match val {
            Some(v) => {
                merged.insert(name.clone(), Some(v.clone()));
            }
            None => {
                merged.remove(name);
            }
        }
    }

    let traits: Vec<IdentityTraitPayload> = merged
        .into_iter()
        .map(|(name, value)| IdentityTraitPayload { name, value })
        .collect();

    ctx.client
        .put(format!("/identities/{identity_id}"), traits)?;
    let updated = ctx
        .client
        .get::<IdentityWithTraits>(format!("/identities/{identity_id}"))?;
    describe(&updated);
    ctx.pending_traits.clear();
    ctx.identity = Some(updated);
    Ok(())
}

/// Drop all staged trait changes for the current identity.
pub fn discard(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.pending_traits.is_empty() {
        println!("No pending changes.");
    } else {
        ctx.pending_traits.clear();
        println!("Pending changes discarded.");
    }
    Ok(())
}
