use rustyline::Context;
use rustyline::hint::{Hint, Hinter};

use super::command::ReplCommand;

pub struct ReplHinter<'a, T> {
    hints: &'a Vec<ReplCommand<T>>,
}

#[derive(Hash, Debug, PartialEq, Eq)]
pub struct CommandHint {
    display: String,
}

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }
    fn completion(&self) -> Option<&str> {
        None
    }
}

impl<T> Hinter for ReplHinter<'_, T> {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if line.is_empty() || line.len() < 3 || pos < line.len() || !line.ends_with(' ') {
            return None;
        }

        let slices = line.split_whitespace().collect::<Vec<_>>();
        let command = self
            .hints
            .iter()
            .find(|candidate| candidate.matches_slices(&slices));

        if let Some(command) = command {
            return Some(CommandHint {
                display: command.remaining_hint(&slices).into(),
            });
        }
        None
    }
}

impl<'a, T> ReplHinter<'a, T> {
    pub fn new(hints: &'a Vec<ReplCommand<T>>) -> ReplHinter<'a, T> {
        ReplHinter { hints }
    }
}
