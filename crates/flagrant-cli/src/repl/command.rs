use anyhow::bail;

use flagrant_types::{Environment, Feature, NewEnvRequestPayload, NewFeatureRequestPayload};

use super::ReplContext;

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
    fn invoke<S: AsRef<str>>(args: Vec<S>, context: &ReplContext) -> anyhow::Result<()>;

    /// Creates a new Command with hint digestable by rustyline
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
    fn invoke<S: AsRef<str>>(args: Vec<S>, context: &ReplContext) -> anyhow::Result<()> {
        if args.is_empty() {
            bail!("Not enough parameters provided.");
        }

        let mut guard = context.lock().unwrap();
        match args.first().map(|s| s.as_ref()).unwrap() {
            "add" => {
                let name = args.get(1);
                let description = args.get(2);

                if let Some(name) = name {
                    let payload = NewEnvRequestPayload {
                        name: name.as_ref().to_owned(),
                        description: description.map(|d| d.as_ref().to_owned()),
                    };
                    let env: Environment = guard.client.post("/envs", payload)?;

                    println!("Created new environment '{}' (id={})", env.name, env.id);
                    return Ok(());
                }
                bail!("No environment name provided")
            }
            "ls" => {
                let envs: Vec<Environment> = guard.client.get("/envs")?;

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
            "sw" => {
                if let Some(name) = args.get(1) {
                    let env: anyhow::Result<Option<Environment>> =
                        guard.client.get(format!("/envs/{}", name.as_ref()));
                    if let Ok(Some(env)) = env {
                        println!("Switched to environment '{}' (id={})", env.name, env.id);
                        guard.environment = Some(env);
                        return Ok(());
                    }
                }
                bail!("No such environment")
            }
            _ => bail!("Unknown subcommand"),
        }
    }
}

impl Invokable for Feat {
    fn triggered_by() -> &'static str {
        "feat"
    }

    fn invoke<S: AsRef<str>>(args: Vec<S>, context: &ReplContext) -> anyhow::Result<()> {
        if args.is_empty() {
            bail!("Not enough parameters provided.");
        }

        let client = &context.lock().unwrap().client;
        let command = args.first().map(|s| s.as_ref()).unwrap();

        match command {
            "add" => {
                if let Some(name) = args.get(1) {
                    if let Some(value) = args.get(2) {
                        let payload = NewFeatureRequestPayload {
                            name: name.as_ref().to_owned(),
                            value: value.as_ref().to_owned(),
                            description: args.get(3).map(|d| d.as_ref().to_owned()),
                            is_enabled: false,
                        };
                        let feat: Feature = client.post("/features", payload)?;

                        println!(
                            "Created new feature '{}' (id={}, value={})",
                            feat.name, feat.id, feat.value
                        );
                        return Ok(());
                    }
                }
                bail!("No feature name or value provided")
            }
            "ls" => {
                let feats: Vec<Feature> = client.get("/features")?;

                println!("{:-^60}", "");
                println!(
                    "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
                    "id", "name", "enabled?", "value"
                );
                println!("{:-^60}", "");

                for feat in feats {
                    println!(
                        "{0: <4} | {1: <30} | {2: <8} | {3: <30}",
                        feat.id, feat.name, feat.is_enabled, feat.value
                    );
                }
                Ok(())
            }
            "val" => {
                if let Some(name) = args.get(1) {
                    if let Some(value) = args.get(2) {
                        let name = name.as_ref();
                        if let Ok(feature) = client.get::<_, Feature>(format!("/features/{name}")) {
                            let payload = NewFeatureRequestPayload {
                                name: name.to_owned(),
                                value: value.as_ref().to_owned(),
                                description: None,
                                is_enabled: feature.is_enabled,
                            };
                            client.put(format!("/features/{name}"), payload)?;

                            // re-fetch feature to be sure it's updated
                            let feature: Feature = client.get(format!("/features/{name}"))?;
                            println!(
                                "Updated feature (id={}, name={}, value={}, is_enabled={})",
                                feature.id, feature.name, feature.value, feature.is_enabled
                            );
                            return Ok(());
                        }
                        bail!("No such a feature")
                    }
                }
                bail!("No feature name or value provided")
            }
            "on" | "off" => {
                if let Some(name) = args.get(1) {
                    let name = name.as_ref();
                    if let Ok(feature) = client.get::<_, Feature>(format!("/features/{name}")) {
                        let payload = NewFeatureRequestPayload {
                            name: feature.name,
                            value: feature.value,
                            description: None,
                            is_enabled: command == "on",
                        };
                        if client.put(format!("/features/{name}"), payload).is_ok() {
                            // re-fetch feature to be sure it's updated
                            let feature: Feature = client.get(format!("/features/{name}"))?;
                            println!(
                                "Updated feature (id={}, name={}, value={}, is_enabled={})",
                                feature.id, feature.name, feature.value, feature.is_enabled
                            );
                            return Ok(());
                        }
                    }
                    bail!("No such a feature")
                }
                bail!("No feature name provided")
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
