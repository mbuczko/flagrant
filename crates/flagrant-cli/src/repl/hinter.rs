use rustyline::hint::{Hint, Hinter};
use rustyline::Context;

use super::command::Command;

pub struct ReplHinter {
    hints: Vec<Command>,
}

#[derive(Hash, Debug, PartialEq, Eq)]
pub struct CommandHint {
    display: String,
    complete_up_to: usize,
}

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        if self.complete_up_to > 0 {
            return Some(&self.display[..self.complete_up_to])
        }
        None
    }
}

impl Hinter for ReplHinter {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if line.is_empty() || line.len() < 3 || pos < line.len() || !line.ends_with(' ') {
            return None;
        }

        let lowered = line[..line.len() - 1].to_lowercase();
        let argc = line.split_whitespace().count();
        let command = self
            .hints
            .iter()
            .find(|candidate| match candidate.argc {
                0 => lowered == candidate.cmd,
                p => argc >= p && lowered.starts_with(candidate.cmd.as_str()),
            });

        if let Some(command) = command {
            let strip_chars = command
                .hint
                .chars()
                .enumerate()
                .filter(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i)
                .nth(argc - 1)
                .unwrap_or(command.hint.len() - 1);

            return Some(CommandHint {
                display: command.hint[strip_chars + 1..].into(),
                complete_up_to: command.cmd.len().saturating_sub(strip_chars),
            });
        }
        None
    }
}

impl ReplHinter {
    pub fn new(hints: Vec<Command>) -> ReplHinter {
        ReplHinter { hints }
    }
}
