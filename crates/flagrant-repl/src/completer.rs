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
    /// Returns unique command completions that match the input line.
    ///
    /// Filters duplicates by assuming commands are sorted lexicographically and
    /// skipping consecutive identical command names. Matches commands that start
    /// with the input, or returns all commands if the line is empty.
    fn complete_command(&self, line: &str) -> anyhow::Result<(usize, Vec<Pair>)> {
        let mut prev_command_str = "";
        let empty = line.trim().is_empty();
        let pairs = self
            .commands
            .iter()
            .filter_map(|(command_str, _, within_ctx)| {
                if command_str != prev_command_str
                    && (empty || command_str.starts_with(line))
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

        Ok((0, pairs))
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
                if command == command_str && within_ctx.as_ref().is_none_or(|f| f()) {
                    return match op {
                        // op starts with prefix - candidate for completion
                        Some(op) if op.starts_with(prefix) => Some(Pair {
                            display: op.to_owned(),
                            replacement: op.to_lowercase().to_owned(),
                        }),

                        // there is no op or it doesn't start with op_prefix - reject
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
                    .complete_by_prefix(command, args, arg_number, arg_prefix)?
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

        match args.len() {
            0..=1 => self
                .complete_command(&line.to_uppercase())
                .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string()))),
            n if arg_n < n => {
                let command = args.first().unwrap().as_ref();
                let argument = &args[arg_n];

                if let Ok(candidates) =
                    self.complete_operation(command, &argument[..offset].to_lowercase(), argument.1)
                    && !candidates.1.is_empty()
                {
                    return Ok(candidates);
                }

                self.complete_argument(command, &args, arg_n, &argument[..offset], argument.1)
                    .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())))
            }
            _ => Ok((pos, vec![])),
        }
    }
}
