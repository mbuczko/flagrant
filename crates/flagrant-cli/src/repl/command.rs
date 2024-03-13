use anyhow::{anyhow, bail};
use flagrant::models::environment;
use futures::executor;

use super::context::ReplContext;

#[derive(Debug)]
pub struct Command {
    pub cmd: String,
    pub hint: String,
    pub argc: usize,
}

pub trait Invokable {
    /// A case-insensitive command which triggers invokable action
    fn triggered_by() -> &'static str;

    /// Invokes a code handling action that it was implemented for.
    ///
    /// Based on provided arguments, function might mutate a context, which
    /// may be helpful in certain situations, like changing current environment.
    fn invoke<S: AsRef<str>>(args: Vec<S>, ctx: &mut ReplContext) -> anyhow::Result<()>;

    fn command(op: Option<&str>, hint: &str) -> Command {
        let op = op.unwrap_or_default();
        let cmd = concat(&[Self::triggered_by(), op]).to_lowercase();
        let hint = concat(&[Self::triggered_by(), op, hint]);
        let mut argc = 0;

        if !op.is_empty() { argc += 1; }

        Command { cmd, hint, argc }
    }
}

pub struct Env;

impl Invokable for Env {
    fn triggered_by() -> &'static str {
        "env"
    }
    fn invoke<S: AsRef<str>>(args: Vec<S>, ctx: &mut ReplContext) -> anyhow::Result<()> {
        if args.is_empty() {
            bail!("Not enough parameters provided.");
        }
        match args.first().map(|s| s.as_ref()).unwrap() {
            "add" => {
                let name = args.get(1);
                let description = args.get(2);
                if let Some(name) = name {
                    let env = executor::block_on(environment::create_environment(
                        &ctx.pool,
                        &ctx.project,
                        name,
                        description,
                    ))?;

                    println!("Created new environment '{}' (id={})", env.name, env.id);
                    return Ok(());
                }
                Err(anyhow!("Environment name not provided"))
            }
            "ls" => {
                let envs = executor::block_on(environment::fetch_environments_for_project(
                    &ctx.pool,
                    &ctx.project,
                ))?;
                for env in envs {
                    println!("{:4} | {}", env.id, env.name);
                }
                Ok(())
            }
            "sw" => {
                if let Some(name) = args.get(1) {
                    let env = executor::block_on(environment::fetch_environment_by_name(
                        &ctx.pool,
                        &ctx.project,
                        name,
                    ))?;
                    if let Some(env) = env {
                        println!("Switched to environment '{}' (id={})", env.name, env.id);
                        return ctx.set_environment(env);
                    } else {
                        bail!("No environment found");
                    }
                }
                Err(anyhow!("Environment name not provided"))
            }
            _ => bail!("Unknown subcommand"),
        }
    }
}

fn concat(strings: &[&str]) -> String {
    strings.iter().fold(String::default(), |acc, s| {
        if s.is_empty() {
            acc
        } else if acc.is_empty() {
            acc + s
        } else {
            acc + " " + s
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let cmd = Env::command(None, "ADD | RM");
        assert_eq!(cmd.cmd, "env");
        assert_eq!(cmd.hint, "env ADD | RM");
    }
}
