use anyhow::{anyhow, bail};
use flagrant::models::environment;
use futures::executor;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Result, Validator};

use super::completer::CommandCompleter;
use super::context::ReplContext;
use super::hinter::{Command, ReplHinter};

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter,
}

pub fn env_actions(args: Vec<&str>, context: &mut ReplContext) -> anyhow::Result<()> {
    if args.is_empty() {
        bail!("Not enough parameters provided.");
    }

    match *args.first().unwrap() {
        "add" => {
            let name = args.get(1);
            let description = args.get(2);
            if let Some(name) = name {
                let fut = environment::create_environment(
                    &context.pool,
                    &context.project,
                    name.to_string(),
                    description.map(|d| d.to_string()),
                );
                let env = executor::block_on(fut)?;
                println!("Created new environment '{}' (id={})", env.name, env.id);
                return Ok(());
            }
            Err(anyhow!("Environment name not provided"))
        }
        "ls" => {
            let fut = environment::fetch_environments_for_project(&context.pool, &context.project);
            let envs = executor::block_on(fut)?;
            for env in envs {
                println!("{:4} | {}", env.id, env.name);
            }
            Ok(())
        }
        "sw" => {
            if let Some(name) = args.get(1) {
                let fut =
                    environment::fetch_environment_by_name(&context.pool, &context.project, name);
                if let Some(env) = executor::block_on(fut)? {
                    println!("Switched to environment '{}' (id={})", env.name, env.id);
                    context.set_environment(env);
                    return Ok(());
                } else {
                    bail!("No environment found");
                }
            }
            Err(anyhow!("Environment name not provided"))
        }
        _ => bail!("Unknown subcommand"),
    }
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init(mut context: ReplContext) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let helper = ReplHelper {
        hinter: ReplHinter::new(vec![
            Command::new("help", "help"),
            Command::new("env", "env ADD | DEL | LS | REN | SW"),
            Command::new("env ADD", "env ADD name description"),
            Command::new("env DEL", "env DEL name"),
            Command::new("env SW", "env SW name"),
            Command::new("env REN", "env RENAME name"),
            Command::new("feat ADD", "feat ADD feature-name value"),
            Command::new("feat DEL", "feat DEL feature-name"),
            Command::new("feat VAL", "feat VAL feature-name new-value"),
            Command::new("feat DESC", "feat DESC feature-name new-description"),
            Command::new("feat LIST", "feat LIST"),
            Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ]),
        completer: CommandCompleter::new(
            vec!["feat", "env"],
            context.project.clone(),
            context.pool.clone(),
        ),
    };

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    rl.set_helper(Some(helper));

    loop {
        let project_name = context.project.name.as_str();
        let readline = rl.readline(format!("[{project_name}] > ").as_str());

        match readline {
            Ok(line) => {
                let mut chunks = line.split_whitespace().collect::<Vec<_>>();
                let command = chunks.remove(0).to_lowercase();
                let result = match command.as_ref() {
                    "env" => env_actions(chunks, &mut context),
                    _ => Err(anyhow!("Action not supported")),
                };

                if let Err(error) = result {
                    tracing::warn!(?error);
                } else {
                    rl.add_history_entry(line.as_str())?;
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    rl.save_history("history.txt")?;
    Ok(())
}
