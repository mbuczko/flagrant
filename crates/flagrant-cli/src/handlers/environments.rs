//! REPL command handlers for environment management.
//!
//! Each public function corresponds to an `ENVIRONMENT <op>` command:
//!
//! | Command                  | Handler            | Description                                |
//! |--------------------------|--------------------|--------------------------------------------|
//! | `ENVIRONMENT add`        | [`add`]            | Create a new environment in the project.   |
//! | `ENVIRONMENT list`       | [`list`]           | Print all environments in the project.     |
//! | `ENVIRONMENT show`       | [`show`]           | Print details of an environment.           |
//! | `ENVIRONMENT describe`   | [`describe`]       | Update an environment's description.       |
//! | `ENVIRONMENT use`        | [`r#use`]          | Switch the active environment.             |
//!
//! Unlike `FEATURE`/`SEGMENT`, environment mutations apply immediately - there is no
//! patch/`COMMIT`/`DISCARD` staging concept for environments.

use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Environment,
    payload::{NewEnvironmentPayload, UpdateEnvironmentPayload},
};

use crate::{handlers::open_in_editor, printer::tabular::Tabular};

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

    Environment::list(
        ctx.client
            .get::<Vec<Environment>>(res.subpath("/envs"))?
            .as_ref(),
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
/// If omitted, opens `$EDITOR` pre-filled with the environment's current description so
/// it can be edited interactively; leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let desc = match args.get(1) {
        Some(d) => Some(d.to_string()),
        None => {
            let current = ctx.environment.description.as_deref().unwrap_or_default();
            let edited = open_in_editor(current)?;
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

/// Switch the active environment by name.
///
/// Expects args: `<environment>`
///
/// Fetches the environment from the API and stores it in the session so that
/// subsequent `FEATURE` commands operate within it.
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
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
            .get::<Environment>(res.subpath(format!("/envs/{name}")));

        if let Ok(env) = response {
            println!("Switching environment → {}", env.name.bold());
            let feature_name = ctx.feature.as_ref().map(|f| f.name.clone());
            ctx.environment = env;
            ctx.identity = None;
            ctx.identity_patch = None;
            drop(ctx);

            if let Some(name) = feature_name {
                let args = [Arg("", 0), Arg(name.as_str(), 1)];
                super::features::r#use(&args, session)?;
            }
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
