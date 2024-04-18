use std::borrow::Cow::{self, Owned};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Hinter, Validator};
use strum::IntoEnumIterator;

use crate::handlers;

use super::command::Command;
use super::completer::CommandCompleter;
use super::hinter::ReplHinter;
use super::session::ReplSession;
use super::tokenizer::split_command_line;

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
    format!(
        "[{}/\x1b[35m{}\x1b[0m] > ",
        ssn.project.name, ssn.environment.name
    )
}

/// Initializes a REPL with history, hints and autocompletions pulled straight
/// from application API in context of respective command.
pub fn init(session: ReplSession) -> anyhow::Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let commands = vec![
        // environments
        Command::Environment.no_op("add | del | set | ls"),
        Command::Environment.op("add", "env-name description", handlers::env::add),
        Command::Environment.op("set", "env-name", handlers::env::switch),
        Command::Environment.op("ls", "", handlers::env::list),
        // features
        Command::Feature.no_op("add | del | ls | val | on | off"),
        Command::Feature.op("ls", "", handlers::feat::list),
        Command::Feature.op("add", "feature-name [value] [text | json | toml]", handlers::feat::add),
        Command::Feature.op("val", "feature-name new-value", handlers::feat::value),
        Command::Feature.op("on", "feature-name", handlers::feat::on),
        Command::Feature.op("off", "feature-name", handlers::feat::off),
        Command::Feature.op("del", "feature-name", handlers::feat::delete),
        // variants
        Command::Variant.no_op("add | del | ls | wgt | val"),
        Command::Variant.op("ls", "feature-name", handlers::var::list),
        Command::Variant.op("add", "feature-name weight value", handlers::var::add),
        Command::Variant.op("del", "variant-id", handlers::var::del),
        Command::Variant.op("wgt", "variant-id new-weight", handlers::var::weight),
        Command::Variant.op("val", "variant-id new-value", handlers::var::value),
    ];
    rl.set_helper(Some(ReplHelper {
        hinter: ReplHinter::new(&commands),
        completer: CommandCompleter::new(
            // collect all variants of Command enum and turn them into
            // lower-cased strings. This is what CommandCompleter expects.
            Command::iter().map(|c| c.to_string()).collect::<Vec<_>>(),
            &session,
        ),
    }));
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    loop {
        match rl.readline(prompt(&session).as_str()) {
            Ok(line) => {

                // after a command line split, all the slices form
                // a vector of following elements:
                //
                // [command, operation, arg, arg, ...]
                //
                // eg: ["ENVIRONMENT", "set", "development"]
                let slices = split_command_line(&line)?;

                if slices.is_empty() {
                    continue;
                }
                if let Some(cmd) = commands
                    .iter()
                    .find(|c| c.matches_slices(&slices))
                {
                    rl.add_history_entry(line.as_str())?;
                    if let Err(error) = (cmd.handler)(&slices[1..], &session) {
                        eprintln!("{error}");
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
