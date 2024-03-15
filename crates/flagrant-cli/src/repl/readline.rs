use anyhow::anyhow;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Validator};

use super::command::{Env, Feat, Invokable};
use super::completer::CommandCompleter;
use super::hinter::ReplHinter;
use super::ReplContext;

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter,
    // #[rustyline(Highlighter)]
    // highlighter: PromptHighlighter
}

pub fn prompt(client: &ReplContext) -> String {
    let guard = client.lock().unwrap();
    let project = &guard.project;
    let env = &guard.environment;

    if let Some(env) = env {
        format!("[{}/{}] > ", project.name, env.name)
    } else {
        format!("[{}] > ", project.name)
    }
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init(context: ReplContext) -> anyhow::Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let helper = ReplHelper {
        hinter: ReplHinter::new(vec![
            // environments
            Env::command(None, "ADD | DEL | LS | SW | REN"),
            Env::command(Some("ADD"), "env-name description"),
            Env::command(Some("DEL"), "env-name"),
            Env::command(Some("REN"), "env-name new-name"),
            Env::command(Some("SW"), "env-name"),

            // features
            Feat::command(None, "ADD | DEL | LS | VAL | ON | OFF"),
            Feat::command(Some("ADD"), "feature-name value"),
            Feat::command(Some("DEL"), "feature-name"),
            Feat::command(Some("VAL"), "feature-name new-value"),
            Feat::command(Some("ON"), "feature-name"),
            Feat::command(Some("OFF"), "feature-name"),

            // Command::new("env REN", "env RENAME name"),
            // Command::new("feat ADD", "feat ADD feature-name value"),
            // Command::new("feat DEL", "feat DEL feature-name"),
            // Command::new("feat VAL", "feat VAL feature-name new-value"),
            // Command::new("feat DESC", "feat DESC feature-name new-description"),
            // Command::new("feat LIST", "feat LIST"),
            // Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ]),
        completer: CommandCompleter::new(vec!["feat", "env"], context.clone()),
    };

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    rl.set_helper(Some(helper));

    loop {
        match rl.readline(prompt(&context).as_str()) {
            Ok(line) => {
                let mut chunks = line.split_whitespace().collect::<Vec<_>>();
                let command = chunks.remove(0).to_lowercase();
                let result = match command.as_ref() {
                    "env" => Env::invoke(chunks, &context),
                    "feat" => Feat::invoke(chunks, &context),
                    _ => Err(anyhow!("Action not supported")),
                };

                if let Err(error) = result {
                    eprintln!("{error}");
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
