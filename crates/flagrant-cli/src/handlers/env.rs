use anyhow::bail;
use ascii_table::AsciiTable;
use flagrant_client::session::{Session, Resource};
use flagrant_types::{EnvRequestPayload, Environment};

use crate::repl::readline::ReplEditor;

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

        println!("Created new environment '{}' (id={})", env.name, env.id);
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

    let mut table = AsciiTable::default();
    let mut vecs = Vec::with_capacity(envs.len() + 1);

    table.column(0).set_header("ID");
    table.column(1).set_header("NAME");
    table.column(2).set_header("DESCRIPTION");

    for env in envs {
        vecs.push(vec![
            env.id.to_string(),
            env.name,
            env.description.unwrap_or_default(),
        ]);
    }
    table.print(vecs);
    Ok(())
}

/// Changes current environment in a session
pub fn switch(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.project.as_base_resource();
        let result = session
            .client
            .get::<Environment>(res.subpath(format!("/envs/name/{name}")));

        if let Ok(env) = result {
            println!("Switching to environment '{}' (id={})", env.name, env.id);
            session.set_environment(env);
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
