use anyhow::{anyhow, bail};
use flagrant::models::environment::{self, Environment};
use flagrant::models::project::Project;
use futures::executor;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Result, Validator};
use sqlx::{Pool, Sqlite};

use super::completer::CommandCompleter;
use super::hinter::{Command, ReplHinter};

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter<'a>,
}

pub fn env_actions(
    args: Vec<&str>,
    project: &Project,
    pool: &Pool<Sqlite>,
) -> anyhow::Result<Environment> {
    if args.len() < 2 {
        bail!("Not enough parameters provided.");
    }

    match *args.first().unwrap() {
        "add" => {
            let name = args.get(1);
            let description = args.get(2).map(|s| s.to_string());

            if let Some(name) = name {
                let fut =
                    environment::create_environment(pool, project, name.to_string(), description);
                let env = executor::block_on(fut)?;

                return Ok(env);
            }
            Err(anyhow!("Environment name not provided"))
        }
        _ => {
            tracing::warn!("Subcommand not provided or not supported");
            bail!("Unknown subcommand")
        }
    }
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init<'a>(project: &'a Project, pool: &'a Pool<Sqlite>) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let helper = ReplHelper {
        hinter: ReplHinter::new(vec![
            Command::new("help", "help"),
            Command::new("env", "env ADD | DEL | LIST | RENAME | SWITCH"),
            Command::new("env ADD", "env ADD name description"),
            Command::new("env DEL", "env DEL name"),
            Command::new("env SWITCH", "env SWITCH name"),
            Command::new("env RENAME", "env RENAME name"),
            Command::new("feat ADD", "feat ADD feature-name value"),
            Command::new("feat DEL", "feat DEL feature-name"),
            Command::new("feat VAL", "feat VAL feature-name new-value"),
            Command::new("feat DESC", "feat DESC feature-name new-description"),
            Command::new("feat LIST", "feat LIST"),
            Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ]),
        completer: CommandCompleter::new(vec!["feat", "env"], project, pool),
    };

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    rl.set_helper(Some(helper));

    loop {
        let project_name = project.name.as_str();
        let readline = rl.readline(format!("[{project_name}] > ").as_str());

        match readline {
            Ok(line) => {
                let mut chunks = line.split_whitespace().collect::<Vec<_>>();
                let command = chunks.remove(0).to_lowercase();
                let result = match command.as_ref() {
                    "env" => env_actions(chunks, project, pool),
                    _ => Err(anyhow!("Action not supported")),
                };

                if let Err(error) = result {
                    tracing::warn!(?error);
                } else {
                    tracing::info!("New environment created ({})", result.unwrap().name);
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
