use flagrant::models::project::Project;
use rustyline::error::ReadlineError;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Context, Editor, Hinter, Helper, Highlighter, Result, Validator};

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
}

struct ReplHinter {
    hints: Vec<CommandHint>,
}

#[derive(Hash, Debug, PartialEq, Eq)]
struct CommandHint {
    display: String,
    complete_up_to: usize,
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

impl CommandHint {
    fn new(text: &str, complete_up_to: &str) -> CommandHint {
        assert!(text.starts_with(complete_up_to));
        CommandHint {
            display: text.into(),
            complete_up_to: complete_up_to.len(),
        }
    }

    fn suffix(&self, strip_chars: usize) -> CommandHint {
        CommandHint {
            display: self.display[strip_chars..].to_owned(),
            complete_up_to: self.complete_up_to.saturating_sub(strip_chars),
        }
    }
}

impl Hinter for ReplHinter {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if line.is_empty() || pos < line.len() || !(line.ends_with(' ') || line.ends_with('/')) {
            return None;
        }

        let cmd = line.split(' ').next().unwrap();
        self.hints
            .iter()
            .filter_map(|hint| {
                if hint.display.starts_with(cmd) {
                    Some(hint.suffix(pos))
                } else {
                    None
                }
            }).next()
    }
}

fn diy_hints() -> Vec<CommandHint> {
    let mut hints = Vec::new();
    hints.push(CommandHint::new("help", "help"));
    hints.push(CommandHint::new("env/[add | del]", "env"));
    hints.push(CommandHint::new("env/add name value", "env/add"));
    hints.push(CommandHint::new("env/del name", "env/del"));
    hints.push(CommandHint::new("hget key field", "hget "));
    hints.push(CommandHint::new("hset key field value", "hset "));
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
