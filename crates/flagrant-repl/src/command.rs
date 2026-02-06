use std::{fmt::Display, ops::Deref};

use super::session::Session;

pub type CommandHandler<T> = fn(&[Arg], &Session<T>) -> anyhow::Result<()>;
pub type CommandInContext<T> = fn(&Session<T>) -> bool;

pub struct ReplCommand<T> {
    pub cmd: String,
    pub op: Option<String>,
    pub hint: String,
    pub handler: CommandHandler<T>,
    pub has_context: Option<CommandInContext<T>>,
}

#[derive(Debug, PartialEq)]
/// A struct extending simple string slice with additional
/// position at which string has been found in command-line
/// during arguments parsing.
pub struct Arg<'a>(pub &'a str, pub usize);

impl Deref for Arg<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Display for Arg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl<T> ReplCommand<T> {
    /// Returns true if Self matches an array of command line slices.
    /// Array is expected to be composed of following elements:
    ///
    /// [COMMAND, operation, arg, arg, ...]
    ///
    /// Matching succeeds only if in-array command and operation match
    /// self's `cmd` and `op` respectively.
    pub fn matches_slices(&self, slices: &[&str]) -> bool {
        !slices.is_empty()
            && slices.first().unwrap().to_uppercase() == self.cmd
            && match (&self.op, slices.get(1)) {
                // command has an op which matches first provided argument
                (Some(op), Some(arg)) => op == arg,
                // command has an op but none has been provided
                (Some(_), None) => false,
                // command has no op - treat all elements as arguments
                (None, _) => true,
            }
    }

    /// Returns remaining part of hint for already entered command-line slices.
    /// Depending on how much slices have been already provided, only a part of
    /// the hint may be returned, eg:
    ///
    ///  - for "COMMAND op" - as no arguments to "op" operation have been provided,
    ///    a hint describing all missing arguments is returned.
    ///  - for "COMMAND op arg" - as first argument ("arg") has been already provided,
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
