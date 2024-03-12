use rustyline::hint::{Hint, Hinter};
use rustyline::Context;

pub struct ReplHinter {
    hints: Vec<Command>,
}

#[derive(Debug)]
struct Command {
    command: String,
    hint: String,
    argc: usize,
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

impl Command {
    fn new(command: &str, hint: &str) -> Command {
        assert!(hint.starts_with(command));
        Command {
            command: command.to_lowercase(),
            hint: hint.into(),
            argc: command.split_whitespace().count() - 1,
        }
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
                0 => lowered == candidate.command,
                p => argc >= p && lowered.starts_with(candidate.command.as_str()),
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
                complete_up_to: command.command.len().saturating_sub(strip_chars),
            });
        }
        None
    }
}

impl ReplHinter {
    pub fn new() -> ReplHinter {
        let hints = vec![
            Command::new("help", "help"),
            Command::new("env", "env ADD | DEL | LIST | RENAME"),
            Command::new("env ADD", "env ADD name"),
            Command::new("env DEL", "env DEL name"),
            Command::new("env RENAME", "env RENAME name"),
            Command::new("feat ADD", "feat ADD feature-name value"),
            Command::new("feat DEL", "feat DEL feature-name"),
            Command::new("feat VAL", "feat VAL feature-name new-value"),
            Command::new(
                "feat DESC",
                "feat DESC feature-name new-description",
            ),
            Command::new("feat LIST", "feat LIST"),
            Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ];
        ReplHinter {
            hints
        }
    }
}
