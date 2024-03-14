use std::io;

use flagrant_types::Environment;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use crate::client::HttpClientContext;

#[derive(Debug)]
pub struct CommandCompleter {
    commands: Vec<&'static str>,
    context: HttpClientContext,
}

impl CommandCompleter {
    /// Completes main commands.
    ///
    /// As for now only main command (like FEAT or ENV) are auto-completed,
    /// which means that subcommands (like ADD) need to be fully entered by hand.
    pub fn complete_command(&self, line: &str) -> anyhow::Result<(usize, Vec<Pair>)> {
        let empty = line.trim().is_empty();
        let pairs = self
            .commands
            .iter()
            .filter_map(|c| {
                if empty || c.starts_with(line) {
                    return Some(Pair {
                        display: c.to_string(),
                        replacement: c.to_uppercase().to_string(),
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
        let guard = self.context.lock().unwrap();
        let project_id = guard.project.id;
        let envs = guard
            .get::<_, Vec<Environment>>(format!("/projects/{project_id}/envs?name={arg_prefix}"))?;
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

    pub fn new(commands: Vec<&'static str>, client: HttpClientContext) -> CommandCompleter {
        Self {
            commands,
            context: client,
        }
    }
}

impl Completer for CommandCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = line.split_whitespace().collect::<Vec<_>>();
        match args.len() {
            // 0 - nothing entered
            // 1 - command not finished with whitespace yet
            // 2 - subcommand not finished with whitespace yet
            0..=2 => self.complete_command(line).map_err(|e| {
                ReadlineError::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
            }),

            // command and subcommand provided - proceed with arg completion.
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
