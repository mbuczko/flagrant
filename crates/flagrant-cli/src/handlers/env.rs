use anyhow::bail;
use flagrant_types::{Environment, NewEnvRequestPayload};

use crate::repl::context::ReplContext;

/// Adds a new Environment
pub fn add<'a>(args: Vec<&'a str>, context: &'a ReplContext) -> anyhow::Result<()> {
    if args.is_empty() {
        bail!("Not enough parameters provided.");
    }
    if let Some(name) = args.get(1) {
        let payload = NewEnvRequestPayload {
            name: name.to_string(),
            description: args.get(2).map(|d| d.to_string()),
        };
        let env: Environment = context.read().unwrap().client.post("/envs", payload)?;
        println!("Created new environment '{}' (id={})", env.name, env.id);
        return Ok(());
    }
    bail!("No environment name provided")
}

/// Lists all environments
pub fn ls<'a>(_args: Vec<&'a str>, context: &'a ReplContext) -> anyhow::Result<()> {
    let envs: Vec<Environment> = context.read().unwrap().client.get("/envs")?;

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

/// Switches REPL context to the other environment
pub fn sw<'a>(args: Vec<&'a str>, context: &'a ReplContext) -> anyhow::Result<()> {
    if args.is_empty() {
        bail!("Not enough parameters provided.");
    }
    if let Some(name) = args.get(1) {
        let env: anyhow::Result<Option<Environment>> = context
            .read()
            .unwrap()
            .client
            .get(format!("/envs/{}", name));
        if let Ok(Some(env)) = env {
            println!("Switched to environment '{}' (id={})", env.name, env.id);
            context.write().unwrap().environment = Some(env);
            return Ok(());
        }
    }
    bail!("No such environment")
}
