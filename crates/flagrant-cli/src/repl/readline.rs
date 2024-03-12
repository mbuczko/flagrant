use flagrant::models::project::Project;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Result, Validator};
use sqlx::{Pool, Sqlite};

use super::hinter::ReplHinter;
use super::completer::CommandCompleter;

#[derive(Helper, Completer, Hinter, Validator, Highlighter)]
struct ReplHelper<'a> {
    #[rustyline(Hinter)]
    hinter: ReplHinter,
    #[rustyline(Completer)]
    completer: CommandCompleter<'a>,
}

pub fn init<'a>(pool: &'a Pool<Sqlite>, project: &'a Project) -> Result<()> {
    let mut rl: Editor<ReplHelper, DefaultHistory> = Editor::new()?;
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }

    let hinter = ReplHinter::new();
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
