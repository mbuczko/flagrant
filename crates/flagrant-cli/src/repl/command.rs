use strum_macros::{EnumIter, EnumString, Display};
use super::session::ReplSession;

type CommandHandler = fn(&[&str], &ReplSession) -> anyhow::Result<()>;

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
    /// Returns true if Self matches an array of command line slices.
    /// Array should be composed of following elements:
    ///
    /// [command, operation, arg, arg, ...]
    ///
    /// Command matches such an array only if command and operation match own
    /// `cmd` and `op` respectively.
    pub fn matches_input_line(&self, slices: &[&str]) -> bool {
        !slices.is_empty()
            && slices.first().unwrap().to_lowercase() == self.cmd
            && match (&self.op, slices.get(1)) {
                // command has an op which matches first provided argument
                (Some(op), Some(arg)) => op == arg,
                // command has an op but none has been provided
                (Some(_), None) => false,
                // command has no op and none was provided
                (None, _) => slices.len() == 1,
            }
    }

    /// Returns a remaining part of hint for already entered command-line slices.
    /// Depending on how much slices have been provided, only a part of the hint
    /// may be returned, eg:
    ///
    /// - for "ENV add" - entire hint describing all arguments is returned.
    /// - for "ENV add dev" - only a part of the hint describing second argument
    ///   is returned.
    pub fn remaining_hint(&self, slices: &[&str]) -> &str {
        let slices_to_skip = match self.op {
            // skip command and op
            Some(_) => 2,
            // skip command only
            None => 1,
        };

        if slices.len() <= slices_to_skip {
            self.hint.as_str()
        } else {
            let strip_chars = self
                .hint
                .chars()
                .enumerate()
                .filter(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i + 1)
                .nth(slices.len() - slices_to_skip - 1);

            if let Some(strip_chars) = strip_chars {
                return &self.hint[strip_chars..];
            }
            ""
        }
    }
}

impl Command {

    fn no_op(_args: &[&str], _session: &ReplSession) -> anyhow::Result<()> {
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
