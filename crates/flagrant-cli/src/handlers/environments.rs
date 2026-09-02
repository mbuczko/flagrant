use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Environment,
    payload::{NewEnvironmentPayload, UpdateEnvironmentPayload},
};

use crate::{
    handlers::prompt_line,
    printer::tabular::{Tabular, environment::list_with_current},
};

/// Create a new environment in the current project.
///
/// Expects args: `<name> [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.project.as_base_resource();
        let env = ctx.client.post::<_, Environment>(
            res.subpath("/envs"),
            NewEnvironmentPayload {
                name: name.to_string(),
                description: None,
                base_env: args.get(2).map(|d| d.to_string()),
            },
        )?;

        env.display(None, &());
        return Ok(());
    }
    bail!("No environment name provided.")
}

/// List all environments in the current project.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project.as_base_resource();

    list_with_current(
        ctx.client
            .get::<Vec<Environment>>(res.subpath("/envs"))?
            .as_ref(),
        Some(&ctx.environment.name),
    );
    Ok(())
}

/// Print details of an environment by name, or the current environment.
///
/// Expected args: `[name]`
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();

    if let Some(name) = args.get(1)
        && name.as_ref() != ctx.environment.name
    {
        let res = ctx.project.as_base_resource();
        let env = ctx
            .client
            .get::<Environment>(res.subpath(format!("/envs/{name}")))?;
        env.display(None, &());
        return Ok(());
    }

    ctx.environment.display(None, &());
    Ok(())
}

/// Update the current environment's description immediately (no staging/`COMMIT`).
///
/// Expected args: `[description]`
///
/// If omitted, prompts inline pre-filled with the environment's current description so
/// it can be edited in place. Leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let desc = match args.get(1) {
        Some(d) => Some(d.to_string()),
        None => {
            let current = ctx.environment.description.as_deref().unwrap_or_default();
            let Some(edited) = prompt_line("New description", current)? else {
                println!("Cancelled.");
                return Ok(());
            };
            let new_desc = (!edited.is_empty()).then_some(edited);

            if new_desc.as_deref() == ctx.environment.description.as_deref() {
                println!("No changes made.");
                return Ok(());
            }
            new_desc
        }
    };

    let res = ctx.project.as_base_resource();
    let env_id = ctx.environment.id;

    ctx.client.put(
        res.subpath(format!("/envs/{env_id}")),
        UpdateEnvironmentPayload {
            description: desc.clone(),
        },
    )?;

    ctx.environment.description = desc;
    ctx.environment.display(None, &());
    Ok(())
}

/// Switch the session into a different environment by name.
///
/// Fetches the environment and stores it in the session so that subsequent `FEATURE`
/// commands operate within it, clears identity context, and re-enters the previously
/// active feature (if any) in the new environment.
///
/// Fails if there are uncommitted staged changes.
pub(crate) fn switch_to(env_name: &str, session: &Session<Connection>) -> anyhow::Result<()> {
    if env_name.is_empty() {
        return hint_available(session);
    }

    let mut ctx = session.context.write().unwrap();
    if ctx
        .feature_patch
        .as_ref()
        .map(|p| !p.is_empty())
        .unwrap_or(false)
    {
        bail!("You have uncommitted changes. Run `COMMIT` or `DISCARD` first.");
    }
    if ctx.has_identity_pending() {
        bail!("You have uncommitted identity changes. Run `COMMIT` or `DISCARD` first.");
    }

    let res = ctx.project.as_base_resource();
    let response = ctx
        .client
        .get::<Environment>(res.subpath(format!("/envs/{env_name}")));

    if let Ok(env) = response {
        println!("Switching environment → {}", env.name.bold());

        let feature_name = ctx.feature.as_ref().map(|f| f.name.clone());
        ctx.environment = env;
        ctx.identity = None;
        ctx.identity_patch = None;
        drop(ctx);

        if let Some(name) = feature_name {
            super::features::enter(&name, session)?;
        }
        return Ok(());
    }
    bail!("No such an environment.")
}

/// Prints every environment name in the current project - shown when `/ENVIRONMENT` is
/// submitted with no environment name.
fn hint_available(session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project.as_base_resource();
    let names = ctx
        .client
        .get::<Vec<Environment>>(res.subpath("/envs"))?
        .into_iter()
        .map(|e| e.name)
        .collect::<Vec<_>>()
        .join(", ");

    println!("Available environments: {}", names.cyan());
    Ok(())
}
