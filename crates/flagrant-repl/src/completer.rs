use std::io;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use super::parser::{find_arg_by_position, split_command_line};

pub struct CommandCompleter<'a> {
    commands: Vec<(String, &'a Option<String>)>,
    arguments: Option<&'a dyn AutoCompleter>,
}

pub trait AutoCompleter {
    fn complete_by_prefix(&self, command: &str, prefix: &str) -> anyhow::Result<Vec<String>>;
}

impl<'a> CommandCompleter<'a> {
    /// Returns a collection of possible (and unique) command completions.
    /// To filter out duplicated assumes that commands are provided in
    /// lexicographical order.
    fn complete_command(&self, line: &str) -> anyhow::Result<(usize, Vec<Pair>)> {
        let mut prev_command_str = "";

        let empty = line.trim().is_empty();
        let pairs = self
            .commands
            .iter()
            .filter_map(|(command_str, _)| {
                if command_str != prev_command_str && (empty || command_str.starts_with(line)) {
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

    /// Returns a collection of possible operation completions for given command
    fn complete_operation(
        &self,
        command: &str,
        op_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        let pairs = self
            .commands
            .iter()
            .filter_map(|(command_str, op)| {
                if command == command_str {
                    return match op {
                        // op starts with prefix - candidate for completion
                        Some(op) if op.starts_with(op_prefix) => Some(Pair {
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

    /// Returns a collection of possible completions for command arguments
    fn complete_argument(
        &self,
        command: &str,
        arg_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        Ok((
            pos,
            match self.arguments {
                Some(arg_completer) => arg_completer
                    .complete_by_prefix(command, arg_prefix)?
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
        self.arguments = Some(completer);
        self
    }

    pub fn new(commands: Vec<(String, &'a Option<String>)>) -> CommandCompleter<'a> {
        Self {
            commands,
            arguments: None,
        }
    }
}

impl Completer for CommandCompleter<'_> {
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

                self.complete_argument(command, &argument[..offset], argument.1)
                    .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())))
            }
            _ => Ok((pos, vec![])),
        }
    }
}
