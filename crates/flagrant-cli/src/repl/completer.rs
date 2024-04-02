use std::io;

use flagrant_types::Environment;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use super::context::ReplContext;
use super::tokenizer::split;

#[derive(Debug)]
pub struct CommandCompleter {
    commands: Vec<&'static str>,
    context: ReplContext,
}

impl CommandCompleter {
    /// Completes main commands.
    ///
    /// As for now only command (like FEAT or ENV) are auto-completed, operations are not.
    pub fn complete_command(&self, line: &str) -> anyhow::Result<(usize, Vec<Pair>)> {
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

    /// Completes contextual arguments to main command (eg. environments names)
    pub fn complete_argument(
        &self,
        _command: &str,
        arg_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        let client = &self.context.read().unwrap().client;
        let envs = client.get::<_, Vec<Environment>>(format!("/envs?name={arg_prefix}"))?;
        let skip_chars = arg_prefix.len() - 1;
        let pairs = envs
            .into_iter()
            .map(|c| Pair {
                replacement: c.name[skip_chars..].to_string(),
                display: c.name,
            })
            .collect::<Vec<_>>();

        Ok((pos, pairs))
    }

    pub fn new(commands: Vec<&'static str>, context: ReplContext) -> CommandCompleter {
        Self {
            commands,
            context,
        }
    }
}

impl Completer for CommandCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = split(line).unwrap();
        match args.len() {
            // 0 - nothing entered
            // 1 - command not finished with whitespace yet
            // 2 - operation not finished with whitespace yet
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
