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
//! | `UNSET trait <name>`           | [`unset_trait`] | Stage a trait removal for the current identity.     |
//! | `COMMIT`                       | [`commit`]      | Send staged trait changes to the API.               |
//! | `DISCARD`                      | [`discard`]     | Drop all staged trait changes.                      |

use anyhow::bail;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    IdentityWithTraits, TraitValue,
    payload::{IdentityRequestPayload, IdentityTraitPayload},
};

use crate::{handlers::internal::stage, printer::tabular::Tabular};

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
        identity.describe(None);
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(identity) = &ctx.identity {
            identity.describe(ctx.identity_patch.as_ref().filter(|p| !p.is_empty()));
        } else {
            bail!("Not in an identity context. Set the context with: \"IDENTITY use\" command.")
        }
    }
    Ok(())
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
        let res = ctx.project.as_base_resource();
        let identity = ctx.client.post::<_, IdentityWithTraits>(
            res.subpath("/identities"),
            IdentityRequestPayload {
                identity: identity_str.to_string(),
                traits: if trait_payloads.is_empty() {
                    None
                } else {
                    Some(trait_payloads)
                },
            },
        )?;
        identity.describe(None);
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

        identity.describe(None);
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

/// Commit staged trait changes for the current identity to the API.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.identity.is_none() {
        bail!("Not in an identity context.");
    }

    let patch = match ctx.identity_patch.take() {
        Some(p) if !p.is_empty() => p,
        _ => {
            println!("No pending changes to commit.");
            return Ok(());
        }
    };

    let identity = ctx.identity.as_ref().unwrap().value.clone();
    let res = ctx.project.as_base_resource();

    match ctx
        .client
        .patch::<_, IdentityWithTraits>(res.subpath(format!("/identities/{identity}")), patch)
    {
        Ok(updated) => {
            updated.describe(None);
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
    } else {
        println!("No pending changes.");
    }
    Ok(())
}

fn resolve_identity(
    ctx: &flagrant_client::connection::Connection,
    identity_str: &str,
) -> anyhow::Result<IdentityWithTraits> {
    let res = ctx.project.as_base_resource();
    ctx.client
        .get::<IdentityWithTraits>(res.subpath(format!("/identities/{identity_str}")))
}
