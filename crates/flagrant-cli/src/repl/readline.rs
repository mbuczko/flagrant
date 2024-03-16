use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Validator};

use crate::handlers;

use super::command::{self, Command, Env, Feat, Var};
use super::completer::CommandCompleter;
use super::context::ReplContext;
use super::hinter::ReplHinter;

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter<'a>,
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
    let commands = vec![
        // environments
        Env::command(None, "add | del | ls | sw", command::no_op),
        Env::command(Some("add"), "env-name description", handlers::env::add),
        Env::command(Some("ls"), "", handlers::env::ls),
        Env::command(Some("sw"), "env-name", handlers::env::sw),

        // features
        Feat::command(None, "all | del | ls | val | on | off", command::no_op),
        Feat::command(Some("add "), "feature-name value", handlers::feat::add),
        Feat::command(Some("val"), "feature-name new-value", handlers::feat::val),
        Feat::command(Some("ls"), "feature-name value", handlers::feat::ls),
        Feat::command(Some("on"), "feature-name", handlers::feat::on),
        Feat::command(Some("off"), "feature-name", handlers::feat::off),

        // Variants
        Var::command(None, "add | del", command::no_op),
        Var::command(Some("add"), "feature-name var-weight var-value", handlers::var::add),
    ];
    let helper = ReplHelper {
        hinter: ReplHinter::new(&commands),
        completer: CommandCompleter::new(vec!["feat", "env", "var"], context.clone()),
    };

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    rl.set_helper(Some(helper));

    loop {
        match rl.readline(prompt(&context).as_str()) {
            Ok(line) => {

                // todo: parse the line to handle description as a string in quotes
                let mut chunks = line.split_whitespace().collect::<Vec<_>>();

                let command = chunks.remove(0).to_lowercase();
                let op = chunks.first().unwrap_or(&"");

                // find the command with provided op and invoke its handler
                if let Some(cmd) = commands
                    .iter()
                    .find(|c| c.cmd == command && c.op.as_str() == *op)
                {
                    // handler for a command might not exists
                    if let Some(handler) = cmd.handler {
                        if let Err(error) = handler(chunks, &context) {
                            eprintln!("{error}");
                        } else {
                            rl.add_history_entry(line.as_str())?;
                        }
                    }
                } else {
                    eprintln!("Action or its argument not supported");
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
