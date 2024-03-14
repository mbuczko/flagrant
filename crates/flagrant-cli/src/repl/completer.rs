use std::io;

use flagrant::models::environment;
use flagrant::models::project::Project;
use futures::executor;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};
use sqlx::{Pool, Sqlite};

#[derive(Debug)]
pub struct CommandCompleter {
    commands: Vec<&'static str>,
    project: Project,
    pool: Pool<Sqlite>,
}

impl CommandCompleter {
    /// Completes main commands.
    ///
    /// As for now only main command (like FEAT or ENV) are auto-completed,
    /// which means that subcommands (like ADD) need to be fully entered by hand.
    pub fn complete_command(&self, line: &str) -> Result<(usize, Vec<Pair>)> {
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

    /// Completes contextual arguments to main command (eg. environments names).
    ///
    /// As this operation requires asynchronous connection to underlaying database,
    /// this function blocks till async code gets resolved and comes back with response.
    pub fn complete_argument(
        &self,
        _command: &str,
        arg_prefix: &str,
        pos: usize,
    ) -> Result<(usize, Vec<Pair>)> {
        let future =
            environment::fetch_environment_by_pattern(&self.pool, &self.project, arg_prefix);
        let skip_chars = arg_prefix.len() - 1;
        let pairs = executor::block_on(future)
            .map_err(|e| ReadlineError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?
            .into_iter()
            .map(|c| Pair {
                replacement: c.name[skip_chars..].to_string(),
                display: c.name,
            })
            .collect::<Vec<_>>();

        Ok((pos, pairs))
    }

    pub fn new(
        commands: Vec<&'static str>,
        project: Project,
        pool: Pool<Sqlite>,
    ) -> CommandCompleter {
        Self {
            commands,
            project,
            pool,
        }
    }}


impl Completer for CommandCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = line.split_whitespace().collect::<Vec<_>>();
        match args.len() {
            // 0 - nothing entered
            // 1 - command not finished with whitespace yet
            // 2 - subcommand not finished with whitespace yet
            0..=2 => self.complete_command(line),

            // command and subcommand provided.
            // proceed with arg completion.
            _ => {
                // back to the nearest whitespace to find begining of argument
                let mut idx = line[..pos].char_indices();
                while let Some((i, ch)) = idx.next_back() {
                    if ch.is_whitespace() {
                        return self.complete_argument(
                            args.first().unwrap(),
                            &line[i + 1..pos],
                            pos - 1,
                        );
                    }
                }
                Ok((pos, Vec::default()))
            }
        }
    }
}
