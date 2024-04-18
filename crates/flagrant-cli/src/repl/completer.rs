use std::io;

use flagrant_types::{Environment, Feature};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use super::session::{ReplSession, Resource};
use super::tokenizer::split_command_line;

#[derive(Debug)]
pub struct CommandCompleter<'a> {
    commands: Vec<String>,
    session: &'a ReplSession,
}

impl<'a> CommandCompleter<'a> {
    /// Completes REPL commands.
    /// As for now only command names (like FEATURE or ENVIRONMENT) are auto-completed,
    /// operations are not.
    fn complete_command(&self, line: &str) -> anyhow::Result<(usize, Vec<Pair>)> {
        let empty = line.trim().is_empty();
        let pairs = self
            .commands
            .iter()
            .filter_map(|c| {
                if empty || c.starts_with(line) {
                    return Some(Pair {
                        display: c.to_string(),
                        replacement: c.to_uppercase().to_owned(),
                    });
                }
                None
            })
            .collect::<Vec<_>>();

        Ok((0, pairs))
    }

    /// Completes contextual arguments to commands (eg. environments for ENVIRONMENT
    /// command or feature names for FEATURE command).
    fn complete_argument(
        &self,
        command: &str,
        arg_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        let ssn = self.session.borrow();
        let skip = arg_prefix.len() - 1;
        let pairs = match command.to_lowercase().as_str() {

            // auto-complete environment names
            "environment" => {
                let res = ssn.project.as_base_resource();
                ssn
                    .client
                    .get::<Vec<Environment>>(res.subpath(format!("/envs?prefix={arg_prefix}")))?
                    .into_iter()
                    .map(|c| Pair {
                        replacement: c.name[skip..].to_string(),
                        display: c.name,
                    })
                    .collect::<Vec<_>>()
            },

            // auto-complete feature name both for "feature" and "variant" commands
            "feature" | "variant" => {
                let res = ssn.environment.as_base_resource();
                ssn
                    .client
                    .get::<Vec<Feature>>(res.subpath(format!("/features?prefix={arg_prefix}")))?
                    .into_iter()
                    .map(|c| Pair {
                        replacement: c.name[skip..].to_string(),
                        display: c.name,
                    })
                    .collect::<Vec<_>>()
            },
            _ => vec![]
        };

        Ok((pos, pairs))
    }

    pub fn new(commands: Vec<String>, session: &'a ReplSession) -> CommandCompleter<'a> {
        Self { commands, session }
    }
}

impl<'a> Completer for CommandCompleter<'a> {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = split_command_line(line).unwrap();
        match args.len() {
            // 0 - nothing entered
            // 1 - command provided, but not finished with whitespace yet
            // 2 - operation provided, but not finished with whitespace yet
            0..=2 => self.complete_command(line).map_err(|e| {
                ReadlineError::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
            }),

            // command and operation provided - proceed with arg completion.
            _ => {
                // back to the nearest whitespace to find begining of argument
                let mut idx = line[..pos].char_indices();
                while let Some((i, ch)) = idx.next_back() {
                    if ch.is_whitespace() {
                        let fut = self.complete_argument(
                            args.first().unwrap(),
                            &line[i + 1..pos],
                            pos - 1,
                        );
                        return fut.map_err(|e| {
                            ReadlineError::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
                        });
                    }
                }
                Ok((pos, Vec::default()))
            }
        }
    }
}
