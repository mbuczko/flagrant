use strum_macros::{EnumIter, EnumString, Display};
use super::{readline::ReplEditor, session::ReplSession};

type CommandHandler = fn(&[&str], &ReplSession, &mut ReplEditor) -> anyhow::Result<()>;

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
    /// Array is expected to be composed of following elements:
    ///
    /// [command, operation, arg, arg, ...]
    ///
    /// Matching succeeds only if in-array command and operation match
    /// self's `cmd` and `op` respectively.
    pub fn matches_slices(&self, slices: &[&str]) -> bool {
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

    /// Returns remaining part of hint for already entered command-line slices.
    /// Depending on how much slices have been already provided, only a part of
    /// the hint may be returned, eg:
    ///
    ///  - for "ENV add" - as no arguments to "add" operation have been provided,
    ///    a hint describing all missing arguments is returned.
    ///  - for "ENV add dev" - as first argument ("dev") has been already provided,
    ///    only a part of the hint describing second argument gets returned.
    pub fn remaining_hint(&self, slices: &[&str]) -> &str {

        // This is to deduce how many slices to ignore initially. Command is skipped
        // in every case, operation is skipped only when it's available.
        let slices_to_skip = match self.op {
            // skip command and op
            Some(_) => 2,
            // no op, skip command only
            None => 1,
        };

        if slices.len() <= slices_to_skip {
            self.hint.as_str()
        } else {
            let strip_chars = self
                .hint
                .char_indices()
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

    /// No-op command handler used to ignore commands called with no required arguments.
    fn no_op_handler(_args: &[&str], _session: &ReplSession, _editor: &mut ReplEditor) -> anyhow::Result<()> {
        Ok(())
    }

    /// Generic command builder.
    /// Creates a new Command with or without operation and command handler function.
    fn build(&self, op: Option<&str>, hint: &str, handler: Option<CommandHandler>) -> ReplCommand {
        ReplCommand {
            cmd: self.to_string(),
            op: op.map(String::from),
            hint: hint.to_owned(),
            handler: handler.unwrap_or(Self::no_op_handler)
        }
    }

    /// Builds a command handling provided operation with `handler` function.
    pub fn op(&self, op: &str, hint: &str, handler: CommandHandler) -> ReplCommand {
        self.build(Some(op), hint, Some(handler))
    }

    /// Builds a no-op (no-operation) version of command.
    /// When invoked, command will be handled by `no_op_handler` which does nothing.
    pub fn no_op(&self, hint: &str) -> ReplCommand {
        self.build(None, hint, None)
    }
}
