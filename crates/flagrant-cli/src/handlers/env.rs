use anyhow::bail;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Environment, payload::EnvRequestPayload};

use crate::printer::tabular::Tabular;

/// Adds a new Environment
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.project.as_base_resource();
        let env = ctx.client.post::<_, Environment>(
            res.subpath("/envs"),
            EnvRequestPayload {
                name: name.to_string(),
                description: args.get(2).map(|d| d.to_string()),
            },
        )?;

        env.describe();
        return Ok(());
    }
    bail!("No environment name provided.")
}

/// Lists all environments
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

/// Changes current environment in a session
pub fn switch(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let mut ctx = session.context.write().unwrap();
        let res = ctx.project.as_base_resource();
        let response = ctx
            .client
            .get::<Environment>(res.subpath(format!("/envs/name/{name}")));

        if let Ok(env) = response {
            println!("Switching to environment '{}' (id={})", env.name, env.id);
            ctx.environment = env;
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
