use std::{
    borrow::Cow::{self, Owned},
    ops::Deref,
};

use rustyline::{
    Completer, Editor, Helper, Hinter, Overlayer, Validator, error::ReadlineError,
    highlight::Highlighter, history::DefaultHistory, overlay::GenericOverlayer,
};

use crate::{PromptFn, command::ReplCommand, session::Session};

use super::{completer::CommandLineCompleter, hinter::ReplHinter, parser::split_command_line};

pub type ReplEditor<'a, T> = Editor<ReplHelper<'a, T>, DefaultHistory>;

#[derive(Helper, Completer, Hinter, Validator, Overlayer)]
pub struct ReplHelper<'a, T> {
    pub prompter: PromptFn<T>,
    #[rustyline(Hinter)]
    pub hinter: ReplHinter<'a, T>,
    #[rustyline(Completer)]
    pub completer: CommandLineCompleter<'a>,
    #[rustyline(Overlayer)]
    pub overlayer: GenericOverlayer,
}

impl<T> Highlighter for ReplHelper<'_, T> {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned(format!("\x1b[38;5;8m{hint}\x1b[0m"))
    }
}

pub fn init<T>(
    helper: ReplHelper<T>,
    session: &Session<T>,
    commands: &[ReplCommand<T>],
) -> anyhow::Result<()> {
    let mut rl: Editor<ReplHelper<T>, DefaultHistory> = Editor::new()?;
    let prompter = helper.prompter;
    rl.set_helper(Some(helper));

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    loop {
        match rl.readline(prompter(session).as_str()) {
            Ok(line) => {
                let slices = split_command_line(&line)?;

                if slices.is_empty() {
                    continue;
                }

                if let Some(cmd) = commands.iter().find(|c| {
                    c.matches_slices(&slices.iter().map(Deref::deref).collect::<Vec<_>>())
                        && c.has_context.map(|check| check(session)).unwrap_or(true)
                }) {
                    rl.add_history_entry(line.as_str())?;
                    if let Err(error) = (cmd.handler)(&slices[1..], session) {
                        eprintln!("{error}");
                    }
                } else {
                    eprintln!("Command or its arguments not supported");
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
