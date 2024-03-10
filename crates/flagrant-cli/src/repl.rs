use flagrant::models::project::Project;
use rustyline::error::ReadlineError;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Context, Editor, Helper, Highlighter, Hinter, Result, Validator};

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
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
        }
    }
}

impl Hinter for ReplHinter {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if line.is_empty() || pos < line.len() || !line.ends_with(' ') {
            return None;
        }

        let lowered = line[..line.len()-1].to_lowercase();
        let command = self.hints
            .iter()
            .filter(|candidate| {
                lowered.starts_with(candidate.command.as_str())
            })
            .next();

        if let Some(command) = command {
            let typed_words = line.split_whitespace().count();
            let strip_chars = command.hint
                .chars()
                .enumerate()
                .filter(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i)
                .nth(typed_words - 1)
                .unwrap_or(command.hint.len()-1);

            return Some(CommandHint {
                display: command.hint[strip_chars+1..].into(),
                complete_up_to: command.command.len().saturating_sub(strip_chars),
            })
        }
        None
    }
}

fn diy_hints() -> Vec<Command> {
    let mut hints = Vec::new();
    hints.push(Command::new("help", "help"));
    hints.push(Command::new("env", "env ADD | DEL | LIST | RENAME"));
    hints.push(Command::new("env ADD", "env ADD name"));
    hints.push(Command::new("env DEL", "env DEL name"));
    hints.push(Command::new("env RENAME", "env RENAME name"));
    hints.push(Command::new("feat ADD", "feat ADD feature-name value"));
    hints.push(Command::new("feat DEL", "feat DEL feature-name"));
    hints.push(Command::new("feat VAL", "feat VAL feature-name new-value"));
    hints.push(Command::new("feat DESC", "feat DESC feature-name new-description"));
    hints.push(Command::new("feat LIST", "feat LIST"));
    hints.push(Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"));
    hints
}

pub fn init_repl(project: Project) -> Result<()> {
    let hinter = ReplHinter { hints: diy_hints() };
    let helper = ReplHelper { hinter };
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;

    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
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
