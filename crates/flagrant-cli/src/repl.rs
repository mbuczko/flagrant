use std::io;

use flagrant::models::environment;
use flagrant::models::project::Project;
use futures::executor;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Context, Editor, Helper, Highlighter, Hinter, Result, Validator};
use sqlx::{Pool, Sqlite};

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter<'a>,
}

struct ReplHinter {
    hints: Vec<Command>,
}

#[derive(Hash, Debug, PartialEq, Eq)]
struct CommandHint {
    display: String,
    complete_up_to: usize,
}

#[derive(Debug)]
struct Command {
    command: String,
    hint: String,
    argc: usize,
}

#[derive(Debug)]
struct CommandCompleter<'a> {
    pool: &'a Pool<Sqlite>,
    project: &'a Project,
    candidates: Vec<&'static str>,
}

impl<'a> CommandCompleter<'a> {
    pub fn complete_command(&self, line: &str) -> Result<(usize, Vec<Pair>)> {
        let empty = line.trim().is_empty();
        let pairs = self
            .candidates
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

    pub fn complete_argument(
        &self,
        command: &str,
        arg_prefix: &str,
        pos: usize,
    ) -> Result<(usize, Vec<Pair>)> {
        let future = environment::fetch_environment_by_name(arg_prefix, self.pool, self.project);
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
        pool: &'a Pool<Sqlite>,
        project: &'a Project,
    ) -> CommandCompleter<'a> {
        CommandCompleter {
            candidates: commands,
            pool,
            project,
        }
    }
}

impl<'a> Completer for CommandCompleter<'a> {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let args = line.split_whitespace().collect::<Vec<_>>();
        match args.len() {
            // 0 - nothing entered
            // 1 - command not finished with whitespace
            // 2 - subcommand not finished with whitespace
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

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        if self.complete_up_to > 0 {
            Some(&self.display[..self.complete_up_to])
        } else {
            None
        }
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
            .filter(|candidate| match candidate.argc {
                0 => lowered == candidate.command,
                p => argc >= p && lowered.starts_with(candidate.command.as_str()),
            })
            .next();

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

fn repl_hints() -> Vec<Command> {
    let mut hints = Vec::new();
    hints.push(Command::new("help", "help"));
    hints.push(Command::new("env", "env ADD | DEL | LIST | RENAME"));
    hints.push(Command::new("env ADD", "env ADD name"));
    hints.push(Command::new("env DEL", "env DEL name"));
    hints.push(Command::new("env RENAME", "env RENAME name"));
    hints.push(Command::new("feat ADD", "feat ADD feature-name value"));
    hints.push(Command::new("feat DEL", "feat DEL feature-name"));
    hints.push(Command::new("feat VAL", "feat VAL feature-name new-value"));
    hints.push(Command::new(
        "feat DESC",
        "feat DESC feature-name new-description",
    ));
    hints.push(Command::new("feat LIST", "feat LIST"));
    hints.push(Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"));

    hints
}

pub fn init_repl<'a>(pool: &'a Pool<Sqlite>, project: &'a Project) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }

    let hinter = ReplHinter {
        hints: repl_hints(),
    };
    let helper = ReplHelper {
        hinter,
        completer: CommandCompleter::new(vec!["feat", "env"], pool, project),
    };
    rl.set_helper(Some(helper));

    loop {
        let project_name = project.name.as_str();
        let readline = rl.readline(format!("[{project_name}] > ").as_str());
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                println!("Line: {}", line);
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    rl.save_history("history.txt")?;
    Ok(())
}
