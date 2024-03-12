use flagrant::models::project::Project;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Result, Validator};
use sqlx::{Pool, Sqlite};

use super::hinter::{Command, ReplHinter};
use super::completer::CommandCompleter;

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter<'a>,
}

/// Inits a REPL with history, hints and autocompletions
/// pulled straight from database in context of given project.
pub fn init<'a>(project: &'a Project, pool: &'a Pool<Sqlite>) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    let helper = ReplHelper {
        hinter: ReplHinter::new(vec![
            Command::new("help", "help"),
            Command::new("env", "env ADD | DEL | LIST | RENAME"),
            Command::new("env ADD", "env ADD name"),
            Command::new("env DEL", "env DEL name"),
            Command::new("env RENAME", "env RENAME name"),
            Command::new("feat ADD", "feat ADD feature-name value"),
            Command::new("feat DEL", "feat DEL feature-name"),
            Command::new("feat VAL", "feat VAL feature-name new-value"),
            Command::new(
                "feat DESC",
                "feat DESC feature-name new-description",
            ),
            Command::new("feat LIST", "feat LIST"),
            Command::new("feat", "feat ADD | DEL | DESC | LIST | VAL"),
        ]),
        completer: CommandCompleter::new(vec!["feat", "env"], project, pool),
    };

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
