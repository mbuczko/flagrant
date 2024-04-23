use anyhow::bail;
use flagrant_types::{Environment, EnvRequestPayload};

use crate::repl::{readline::ReplEditor, session::{ReplSession, Resource}};

/// Adds a new Environment
pub fn add(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.project.as_base_resource();
        let env = ssn.client.post::<_, Environment>(
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
pub fn list(_args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    let ssn = session.borrow();
    let res = ssn.project.as_base_resource();
    let envs = ssn.client.get::<Vec<Environment>>(res.subpath("/envs"))?;

    println!("{:─^60}", "");
    println!("{0: <4} │ {1: <30} │ DESCRIPTION", "ID", "NAME");
    println!("{:─^60}", "");

    for env in envs {
        println!(
            "{0: <4} │ {1: <30} │ {2: <30}",
            env.id,
            env.name,
            env.description.unwrap_or_default()
        );
    }
    Ok(())
}

/// Changes current environment in a session
pub fn switch(args: &[&str], session: &ReplSession, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let mut ssn = session.borrow_mut();
        let res = ssn.project.as_base_resource();
        let result = ssn.client.get::<Vec<Environment>>(res.subpath(format!("/envs?name={name}")));

        if let Ok(mut envs) = result && !envs.is_empty() {
            let env = envs.remove(0);

            println!("Switching to environment '{}' (id={})", env.name, env.id);
            ssn.switch_environment(env);
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
