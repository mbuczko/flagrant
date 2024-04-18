use strum_macros::{EnumIter, EnumString, Display};
use super::session::ReplSession;

type CommandHandler = fn(Vec<&str>, &ReplSession) -> anyhow::Result<()>;

#[derive(Debug, Display, EnumIter, EnumString)]
pub enum Command {
    #[strum(to_string = "environment")]
    Environment,
    #[strum(to_string = "feature")]
    Feature,
    #[strum(to_string = "variant")]
    Variant,
}

#[derive(Debug)]
pub struct ReplCommand {
    pub cmd: String,
    pub op: Option<String>,
    pub hint: String,
    pub handler: CommandHandler,
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
                // command has an op but none has been provided
                (Some(_), None) => false,
                // command has no op and none was provided
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

impl Command {

    fn no_op(_args: Vec<&str>, _session: &ReplSession) -> anyhow::Result<()> {
        Ok(())
    }

    /// Creates a new Command with hint digestable by rustyline
    pub fn build(&self, op: Option<&str>, hint: &str, handler: Option<CommandHandler>) -> ReplCommand {
        ReplCommand {
            cmd: self.to_string(),
            op: op.map(String::from),
            hint: hint.to_owned(),
            handler: handler.unwrap_or(Self::no_op)
        }
    }
}
