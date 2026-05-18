//! REPL command handlers for environment management.
//!
//! Each public function corresponds to an `ENV <op>` command:
//!
//! | Command       | Handler    | Description                              |
//! |---------------|------------|------------------------------------------|
//! | `ENV add`     | [`add`]    | Create a new environment in the project. |
//! | `ENV list`    | [`list`]   | Print all environments in the project.   |
//! | `ENV use`     | [`r#use`]  | Switch the active environment.           |

use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Environment, payload::NewEnvironmentPayload};

use crate::printer::tabular::Tabular;

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

        env.describe(None);
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
        let res = ctx.project.as_base_resource();
        let response = ctx
            .client
            .get::<Environment>(res.subpath(format!("/envs/{name}")));

        if let Ok(env) = response {
            println!("Switching environment → {}", env.name.bold());
            let feature_name = ctx.feature.as_ref().map(|f| f.name.clone());
            ctx.environment = env;
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
