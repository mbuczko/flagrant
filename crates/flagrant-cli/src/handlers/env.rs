use anyhow::bail;
use flagrant_client::session::{Resource, Session};
use flagrant_types::{payloads::EnvRequestPayload, Environment};

use crate::{repl::readline::ReplEditor, printer::tabular::Tabular};

/// Adds a new Environment
pub fn add(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.project.as_base_resource();
        let env = session.client.post::<_, Environment>(
            res.subpath("/envs"),
            EnvRequestPayload {
                name: name.to_string(),
                description: args.get(2).map(|d| d.to_string()),
            },
        )?;

        env.render();
        return Ok(());
    }
    bail!("No environment name provided.")
}

/// Lists all environments
pub fn list(_args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    let res = session.project.as_base_resource();
    let envs = session
        .client
        .get::<Vec<Environment>>(res.subpath("/envs"))?;

    let mut rows = Vec::with_capacity(envs.len());
    for env in envs {
        rows.push([
            env.id.to_string(),
            env.name,
            env.description.unwrap_or_default(),
        ]);
    }
    Environment::table().render(rows);
    Ok(())
}

/// Changes current environment in a session
pub fn switch(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.project.as_base_resource();
        let response = session
            .client
            .get::<Environment>(res.subpath(format!("/envs/name/{name}")));

        if let Ok(env) = response {
            println!("Switching to environment '{}' (id={})", env.name, env.id);
            session.set_environment(env);
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
