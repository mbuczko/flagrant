use anyhow::anyhow;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Result, Validator};

use super::command::{Env, Invokable};
use super::completer::CommandCompleter;
use super::context::ReplContext;
use super::highlighter::PromptHighlighter;
use super::hinter::ReplHinter;

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter,
    #[rustyline(Highlighter)]
    highlighter: PromptHighlighter
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init(mut context: ReplContext) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let helper = ReplHelper {
        hinter: ReplHinter::new(vec![
            Env::command(None, "ADD | DEL | SW"),
            Env::command(Some("ADD"), "name description"),
            Env::command(Some("DEL"), "name"),
            Env::command(Some("SW"), "name"),
            // Command::new("help", "help"),
            // Command::new("env", "env ADD | DEL | LS | REN | SW"),
            // Command::new("env ADD", "env ADD name description"),
            // Command::new("env DEL", "env DEL name"),
            // Command::new("env SW", "env SW name"),
            // Command::new("env REN", "env RENAME name"),
            // Command::new("feat ADD", "feat ADD feature-name value"),
            // Command::new("feat DEL", "feat DEL feature-name"),
            // Command::new("feat VAL", "feat VAL feature-name new-value"),
            // Command::new("feat DESC", "feat DESC feature-name new-description"),
            // Command::new("feat LIST", "feat LIST"),
            // Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ]),
        completer: CommandCompleter::new(
            vec!["feat", "env"],
            context.project.clone(),
            context.pool.clone(),
        ),
        highlighter: PromptHighlighter::new()
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
                    "env" => Env::invoke(chunks, &mut context),
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
