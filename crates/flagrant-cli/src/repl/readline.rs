use std::borrow::Cow::{self, Owned};

use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Hinter, Validator};

use crate::handlers;

use super::command::{self, Command, Env, Feat, Var};
use super::completer::CommandCompleter;
use super::hinter::ReplHinter;
use super::tokenizer::split;
use super::session::ReplSession;

#[derive(Helper, Completer, Hinter, Validator)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter<'a>,
    #[rustyline(Completer)]
    completer: CommandCompleter<'a>,
}

impl<'a> Highlighter for ReplHelper<'a> {
    /// Hint in a dark gray
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned("\x1b[38;5;8m".to_owned() + hint + "\x1b[0m")
    }
}

pub fn prompt(session: &ReplSession) -> String {
    let ssn = session.borrow();
    format!("[{}/\x1b[35m{}\x1b[0m] > ", ssn.project.name, ssn.environment.name)
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init(session: ReplSession) -> anyhow::Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let commands = vec![
        // environments
        Env::command(None, "add | del | set | ls", command::no_op),
        Env::command(Some("add"), "env-name description", handlers::env::add),
        Env::command(Some("set"), "env-name", handlers::env::switch),
        Env::command(Some("ls"), "", handlers::env::list),
        // features
        Feat::command(None, "add | del | ls | val | on | off", command::no_op),
        Feat::command(Some("ls"), "", handlers::feat::list),
        Feat::command(Some("add"), "feature-name [value] [text | json | toml]", handlers::feat::add),
        Feat::command(Some("val"), "feature-name new-value", handlers::feat::value),
        Feat::command(Some("on"), "feature-name", handlers::feat::on),
        Feat::command(Some("off"), "feature-name", handlers::feat::off),
        Feat::command(Some("del"), "feature-name", handlers::feat::delete),
        // variants
        Var::command(None, "add | del | ls | wgt | val", command::no_op),
        Var::command(Some("ls"), "feature-name", handlers::var::list),
        Var::command(Some("add"), "feature-name weight value", handlers::var::add),
        Var::command(Some("del"), "variant-id", handlers::var::del),
        Var::command(Some("wgt"), "variant-id new-weight", handlers::var::weight),
        Var::command(Some("val"), "variant-id new-value", handlers::var::value),
    ];
    rl.set_helper(Some(ReplHelper {
        hinter: ReplHinter::new(&commands),
        completer: CommandCompleter::new(vec!["feature", "environment", "variant"], &session),
    }));
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    loop {
        match rl.readline(prompt(&session).as_str()) {
            Ok(line) => {
                let mut words = split(&line)?;
                if words.is_empty() {
                    continue;
                }
                let command = words.remove(0).to_lowercase();
                let op = words.first().as_ref().map(|&s| *s);

                // find the command with provided op and invoke its handler
                if let Some(cmd) = commands
                    .iter()
                    .find(|c| c.cmd == command && c.op.as_deref() == op)
                {
                    // handler for a command might not exists
                    if let Some(handler) = cmd.handler {
                        rl.add_history_entry(line.as_str())?;
                        if let Err(error) = handler(words, &session) {
                            eprintln!("{error}");
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
