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
    pub op: Option<String>,
    pub hint: String,
    pub handler: Option<CommandHandler>,
}

impl ReplCommand {
    /// Returns true if command matches provided array of command line words.
    /// If not empty, array of words contains command (like 'ENV') as a first
    /// element and optionally operation (like 'add') as a second one.
    pub fn matches(&self, words: &[&str]) -> bool {
        !words.is_empty()
            && words.first().unwrap().to_lowercase() == self.cmd
            && match (&self.op, words.get(1)) {
                // command has an op which matches first provided argument
                (Some(op), Some(arg)) => op == arg,
                // command has an op but no one has been provided
                (Some(_), None) => false,
                // command has no op and nothing except the command was provided
                (None, _) => words.len() == 1,
            }
    }

    /// Returns a remaining part of hint for already entered command-line words.
    /// Depending on how much words have been provided, only a part of the hint
    /// may be returned, eg:
    ///
    /// - for "ENV add" - entire hint describing all arguments is returned.
    /// - for "ENV add dev" - only a part of the hint describing second argument
    ///   is returned.
    pub fn remaining_hint(&self, words: &[&str]) -> &str {
        let words_to_skip = match self.op {
            // skip command and op
            Some(_) => 2,
            // skip command only
            None => 1,
        };

        if words.len() <= words_to_skip {
            self.hint.as_str()
        } else {
            let strip_chars = self
                .hint
                .chars()
                .enumerate()
                .filter(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i + 1)
                .nth(words.len() - words_to_skip - 1);

            if let Some(strip_chars) = strip_chars {
                return &self.hint[strip_chars..];
            }
            ""
        }
    }
}

pub trait Command {
    /// A case-insensitive command which triggers invokable action
    fn triggered_by() -> &'static str;

    /// Creates a new Command with hint digestable by rustyline
    fn command(op: Option<&str>, hint: &str, handler: CommandHandler) -> ReplCommand {
        ReplCommand {
            cmd: Self::triggered_by().into(),
            op: op.map(String::from),
            hint: hint.to_owned(),
            handler: Some(handler),
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
