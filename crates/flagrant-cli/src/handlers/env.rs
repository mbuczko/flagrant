use anyhow::bail;
use flagrant_types::{Environment, NewEnvRequestPayload};

use crate::repl::session::{ReplSession, Resource};

/// Adds a new Environment
pub fn add(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.project.as_resource();
        let env = ssn.client.post::<_, Environment>(
            res.to_path("/envs"),
            NewEnvRequestPayload {
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
pub fn list(_args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    let ssn = session.borrow();
    let res = ssn.project.as_resource();
    let envs = ssn.client.get::<Vec<Environment>>(res.to_path("/envs"))?;

    println!("{:-^52}", "");
    println!("{0: <4} | {1: <30} | description", "id", "name");
    println!("{:-^52}", "");

    for env in envs {
        println!(
            "{0: <4} | {1: <30} | {2: <30}",
            env.id,
            env.name,
            env.description.unwrap_or_default()
        );
    }
    Ok(())
}

/// Changes current environment in a session
pub fn switch(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.project.as_resource();
        let result = ssn.client.get::<Vec<Environment>>(res.to_path(format!("/envs?name={name}")));

        if let Ok(mut envs) = result && !envs.is_empty() {
            let env = envs.remove(0);

            println!("Switched to environment '{}' (id={})", env.name, env.id);
            session.borrow_mut().switch_environment(env);
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
