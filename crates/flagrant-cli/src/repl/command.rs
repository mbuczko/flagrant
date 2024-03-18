use super::context::ReplContext;

type CommandHandler = fn(Vec<&str>, &ReplContext) -> anyhow::Result<()>;

/// Feature related commands
pub struct Feat;

/// Variants related commands
pub struct Var;

/// Environment related commands
pub struct Env;

#[derive(Debug)]
pub struct ReplCommand {
    pub cmd: String,
    pub op: String,
    pub hint: String,
    pub handler: Option<CommandHandler>,

    // private fields to speed up lookup for proper command
    _argc: usize,
    _cmdop: String,
}

impl ReplCommand {
    pub fn matches(&self, line: &str) -> bool {
        match self._argc {
            0 => line == self._cmdop,
            p => self._argc >= p && line.starts_with(self._cmdop.as_str()),
        }
    }
}

pub trait Command {
    /// A case-insensitive command which triggers invokable action
    fn triggered_by() -> &'static str;

    /// Creates a new Command with hint digestable by rustyline
    fn command(op: Option<&str>, hint: &str, handler: CommandHandler) -> ReplCommand {
        let op = op.unwrap_or_default();
        let mut argc = 0;

        // op counts as argument to a command
        if !op.is_empty() {
            argc += 1;
        }

        ReplCommand {
            cmd: Self::triggered_by().into(),
            op: op.to_string(),
            hint: concat(&[Self::triggered_by(), op, hint]),
            handler: Some(handler),
            _argc: argc,
            _cmdop: concat(&[Self::triggered_by(), op]).to_lowercase(),
        }
    }
}

impl Command for Env {
    fn triggered_by() -> &'static str {
        "env"
    }
}

impl Command for Feat {
    fn triggered_by() -> &'static str {
        "feat"
    }
}

impl Command for Var {
    fn triggered_by() -> &'static str {
        "var"
    }
}

pub fn no_op(_args: Vec<&str>, _ctx: &ReplContext) -> anyhow::Result<()> {
    Ok(())
}

fn concat(strings: &[&str]) -> String {
    strings.iter().fold(String::default(), |acc, s| {
        if s.is_empty() {
            acc
        } else if acc.is_empty() {
            acc + s
        } else {
            acc + " " + s
        }
    })
}
