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

/// Called with the text following a help-overlay trigger char (e.g. `?FEATURE` yields
/// `"FEATURE"`, a bare `?` yields `""`) and the session, so it can print whatever help
/// text it wants without `flagrant-repl` needing to know what "help" means.
pub type HelpHandler<T> = fn(&str, &Session<T>) -> anyhow::Result<()>;

#[derive(Helper, Completer, Hinter, Validator, Overlayer)]
pub struct ReplHelper<'a, T: 'static> {
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
    help: Option<(char, HelpHandler<T>)>,
    overlay: Option<(char, &[ReplCommand<T>])>,
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
                // Help overlay
                if let Some((trigger, handler)) = help
                    && let Some(rest) = line.strip_prefix(trigger)
                {
                    rl.add_history_entry(line.as_str())?;
                    if let Err(error) = handler(rest.trim(), session) {
                        eprintln!("{error}");
                    }
                    continue;
                }
                // Context overlay
                if let Some((trigger, overlay_cmds)) = overlay
                    && let Some(rest) = line.strip_prefix(trigger)
                {
                    let slices = split_command_line(rest)?;
                    rl.add_history_entry(line.as_str())?;

                    if slices.is_empty() {
                        let names = overlay_cmds
                            .iter()
                            .map(|c| c.cmd.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        println!("Available commands: {names}");
                    } else if let Some(cmd) = overlay_cmds.iter().find(|c| {
                        c.matches_slices(&slices.iter().map(Deref::deref).collect::<Vec<_>>())
                    }) {
                        if let Err(error) = (cmd.handler)(&slices[1..], session) {
                            eprintln!("{error}");
                        } else {
                            rl.escape_overlay();
                        }
                    } else {
                        eprintln!("Command or its arguments not supported");
                    }
                    continue;
                }

                let slices = split_command_line(&line)?;

                if slices.is_empty() {
                    continue;
                }

                if let Some(cmd) = commands.iter().find(|c| {
                    c.matches_slices(&slices.iter().map(Deref::deref).collect::<Vec<_>>())
                        && c.has_context
                            .map(|checks| checks.iter().all(|check| check(session)))
                            .unwrap_or(true)
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
