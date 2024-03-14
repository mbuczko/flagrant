use anyhow::{anyhow, bail};

use crate::client::HttpClientContext;
use flagrant_types::{NewEnvRequestPayload, Environment};

#[derive(Debug)]
pub struct Command {
    pub cmd: String,
    pub hint: String,
    pub argc: usize,
}

pub trait Invokable {
    /// A case-insensitive command which triggers invokable action
    fn triggered_by() -> &'static str;

    /// Invokes machinery handling an action that it was implemented for.
    fn invoke<S: AsRef<str>>(args: Vec<S>, client: &HttpClientContext) -> anyhow::Result<()>;

    fn command(op: Option<&str>, hint: &str) -> Command {
        let op = op.unwrap_or_default();
        let cmd = concat(&[Self::triggered_by(), op]).to_lowercase();
        let hint = concat(&[Self::triggered_by(), op, hint]);
        let mut argc = 0;

        if !op.is_empty() {
            argc += 1;
        }

        Command { cmd, hint, argc }
    }
}

pub struct Env;
pub struct Feat;

impl Invokable for Env {
    fn triggered_by() -> &'static str {
        "env"
    }
    fn invoke<S: AsRef<str>>(args: Vec<S>, client: &HttpClientContext) -> anyhow::Result<()> {
        if args.is_empty() {
            bail!("Not enough parameters provided.");
        }

        let project_id = client.lock().unwrap().project.id;
        match args.first().map(|s| s.as_ref()).unwrap() {
            "add" => {
                let name = args.get(1);
                let description = args.get(2);

                if let Some(name) = name {
                    let payload = NewEnvRequestPayload {
                        name: name.as_ref().to_owned(),
                        description: description.map(|d| d.as_ref().to_owned()),
                    };
                    let env: Environment =
                        client.lock().unwrap().post(format!("/projects/{project_id}/envs"), &payload)?;

                    println!("Created new environment '{}' (id={})", env.name, env.id);
                    return Ok(());
                }
                Err(anyhow!("Environment name not provided"))
            }
            "ls" => {
                let envs: Vec<Environment> = client.lock().unwrap().get(format!("/projects/{project_id}/envs"))?;
                for env in envs {
                    println!("{:4} | {}", env.id, env.name);
                }
                Ok(())
            }
            "sw" => {
                if let Some(name) = args.get(1) {
                    let env: Option<Environment> = client.lock().unwrap().get(format!("/projects/{project_id}/envs/{}", name.as_ref()))?;

                    if let Some(env) = env {
                        println!("Switched to environment '{}' (id={})", env.name, env.id);
                        client.lock().unwrap().environment = Some(env);
                        return Ok(());
                    }
                    bail!("No environment found")
                }
                Err(anyhow!("Environment name not provided"))
            }
            _ => bail!("Unknown subcommand"),
        }
    }
}

impl Invokable for Feat {
    fn triggered_by() -> &'static str {
        "feat"
    }

    fn invoke<S: AsRef<str>>(_args: Vec<S>, _client: &HttpClientContext) -> anyhow::Result<()> {
        todo!()
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
