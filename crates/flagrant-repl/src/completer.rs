use std::io;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use crate::command::Arg;

use super::parser::{find_arg_by_position, split_command_line};

/// A list of commands with their optional operations.
/// Each entry is a tuple of (command_name, optional_operation, context_checker).
pub type CommandList<'a> = Vec<(
    String,
    &'a Option<String>,
    Option<Box<dyn Fn() -> bool + 'a>>,
)>;

pub struct CommandLineCompleter<'a> {
    commands: CommandList<'a>,
    arg_completer: Option<&'a dyn AutoCompleter>,
}

pub trait AutoCompleter {
    /// Returns possible completions for a command argument at a specific position.
    ///
    /// Delegates to the registered `AutoCompleter` to generate context-aware suggestions
    /// based on the command name, argument position, and the partial text already typed.
    ///
    /// # Arguments
    /// * `command` - The command being completed (e.g., "feature", "environment")
    /// * `args` - All parsed arguments from the command line
    /// * `arg_number` - Zero-based index of the argument being completed
    /// * `arg_prefix` - The partial text of the argument typed so far
    /// * `pos` - Cursor position in the input line
    ///
    /// # Returns
    /// A tuple of (cursor_position, completion_pairs) where completion_pairs contains
    /// the matching suggestions. Returns an empty list if no completions could be found.
    fn complete_by_prefix(
        &self,
        command: &str,
        args: &[Arg],
        pos: usize,
        prefix: &str,
    ) -> anyhow::Result<Vec<String>>;
}

impl<'a> CommandLineCompleter<'a> {
    /// Returns unique command completions matching the command token's prefix.
    ///
    /// Filters duplicates by assuming commands are sorted lexicographically and
    /// skipping consecutive identical command names. Matches commands that start
    /// with `prefix`, or returns all commands if `prefix` is empty.
    fn complete_command(&self, prefix: &str) -> anyhow::Result<Vec<Pair>> {
        let mut prev_command_str = "";
        let empty = prefix.trim().is_empty();
        let pairs = self
            .commands
            .iter()
            .filter_map(|(command_str, _, within_ctx)| {
                if command_str != prev_command_str
                    && (empty || command_str.starts_with(prefix))
                    && within_ctx.as_ref().is_none_or(|f| f())
                {
                    prev_command_str = command_str;
                    return Some(Pair {
                        display: String::default(),
                        replacement: command_str.to_owned(),
                    });
                }
                None
            })
            .collect::<Vec<_>>();

        Ok(pairs)
    }

    /// Returns operation completions for a specific command that match the given prefix.
    ///
    /// Operations are command-specific actions (e.g., "add", "list") that follow the
    /// command name (like "FEATURE"). Returns matching operations with the display form
    /// preserved and the replacement form lowercased.
    fn complete_operation(
        &self,
        command: &str,
        prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        let pairs = self
            .commands
            .iter()
            .filter_map(|(command_str, op, within_ctx)| {
                if command_str.eq_ignore_ascii_case(command)
                    && within_ctx.as_ref().is_none_or(|f| f())
                {
                    return match op {
                        // Op starts with prefix - candidate for completion
                        Some(op) if op.starts_with(prefix) => Some(Pair {
                            display: op.to_owned(),
                            replacement: op.to_lowercase().to_owned(),
                        }),

                        // No op or it doesn't start with op_prefix - reject
                        _ => None,
                    };
                }
                None
            })
            .collect::<Vec<_>>();

        Ok((pos, pairs))
    }

    /// Returns possible completions for a command argument at a specific position.
    ///
    /// Delegates to the registered `AutoCompleter` to generate context-aware suggestions
    /// based on the command name, argument position, and the partial text already typed.
    /// Returns an empty list if no `AutoCompleter` is registered.
    fn complete_argument(
        &self,
        command: &str,
        args: &[Arg],
        arg_number: usize,
        arg_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        Ok((
            pos,
            match self.arg_completer {
                Some(arg_completer) => arg_completer
                    .complete_by_prefix(command, args, arg_number, arg_prefix)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| Pair {
                        replacement: s,
                        display: String::default(),
                    })
                    .collect::<Vec<_>>(),
                _ => vec![],
            },
        ))
    }

    pub fn with_arg_completer(mut self, completer: &'a dyn AutoCompleter) -> Self {
        self.arg_completer = Some(completer);
        self
    }

    pub fn new(commands: CommandList<'a>) -> CommandLineCompleter<'a> {
        Self {
            commands,
            arg_completer: None,
        }
    }
}

impl Completer for CommandLineCompleter<'_> {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = split_command_line(line).unwrap();
        let (arg_n, offset) = find_arg_by_position(&args, pos);

        // Dispatch on which token the cursor is actually in (arg_n), not on how many
        // tokens the line has - otherwise editing an earlier token (e.g. moving back into
        // "FEATURE" or "use" after the line already has more words typed after it) would
        // fall through to argument completion instead of command/operation completion.
        if arg_n == 0 {
            let start = args.first().map(|a| a.1).unwrap_or(0);
            let prefix = args
                .first()
                .map(|a| a[..offset].to_uppercase())
                .unwrap_or_default();
            return self
                .complete_command(&prefix)
                .map(|pairs| (start, pairs))
                .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())));
        }

        let command = args.first().unwrap().as_ref();
        let argument = &args[arg_n];

        if arg_n == 1
            && let Ok(candidates) =
                self.complete_operation(command, &argument[..offset].to_lowercase(), argument.1)
            && !candidates.1.is_empty()
        {
            return Ok(candidates);
        }

        self.complete_argument(command, &args, arg_n, &argument[..offset], argument.1)
            .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use rustyline::history::DefaultHistory;

    use super::*;

    #[test]
    fn completes_command_when_editing_first_token_with_more_tokens_after() {
        let op = Some("use".to_string());
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let completer = CommandLineCompleter::new(commands);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "FEAT use ui_theme" with the cursor right after "FEAT" - simulates deleting
        // "URE" from "FEATURE" while later tokens are still present on the line.
        let (start, pairs) = completer.complete("FEAT use ui_theme", 4, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(pairs.iter().any(|p| p.replacement == "FEATURE"));
    }

    #[test]
    fn completes_operation_when_editing_second_token_with_more_tokens_after() {
        let op = Some("use".to_string());
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let completer = CommandLineCompleter::new(commands);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "FEATURE us ui_theme" with the cursor right after "us" - simulates deleting
        // "e" from "use" while a 3rd token is still present on the line.
        let (_start, pairs) = completer.complete("FEATURE us ui_theme", 10, &ctx).unwrap();
        assert!(pairs.iter().any(|p| p.replacement == "use"));
    }
}
