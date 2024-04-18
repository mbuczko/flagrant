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
use super::tokenizer::split;

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

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init(session: ReplSession) -> anyhow::Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let commands = vec![
        // environments
        Command::Environment.build(None, "add | del | set | ls", None),
        Command::Environment.build(Some("add"), "env-name description", Some(handlers::env::add)),
        Command::Environment.build(Some("set"), "env-name", Some(handlers::env::switch)),
        Command::Environment.build(Some("ls"), "", Some(handlers::env::list)),
        // features
        Command::Feature.build(None, "add | del | ls | val | on | off", None),
        Command::Feature.build(Some("ls"), "", Some(handlers::feat::list)),
        Command::Feature.build(Some("add"), "feature-name [value] [text | json | toml]", Some(handlers::feat::add)),
        Command::Feature.build(Some("val"), "feature-name new-value", Some(handlers::feat::value)),
        Command::Feature.build(Some("on"), "feature-name", Some(handlers::feat::on)),
        Command::Feature.build(Some("off"), "feature-name", Some(handlers::feat::off)),
        Command::Feature.build(Some("del"), "feature-name", Some(handlers::feat::delete)),
        // variants
        Command::Variant.build(None, "add | del | ls | wgt | val", None),
        Command::Variant.build(Some("ls"), "feature-name", Some(handlers::var::list)),
        Command::Variant.build(Some("add"), "feature-name weight value", Some(handlers::var::add)),
        Command::Variant.build(Some("del"), "variant-id", Some(handlers::var::del)),
        Command::Variant.build(Some("wgt"), "variant-id new-weight", Some(handlers::var::weight)),
        Command::Variant.build(Some("val"), "variant-id new-value", Some(handlers::var::value)),
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
                    rl.add_history_entry(line.as_str())?;
                    if let Err(error) = (cmd.handler)(words, &session) {
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
