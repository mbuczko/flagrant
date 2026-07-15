use std::{fmt::Display, ops::Deref};

use super::session::Session;

pub type CommandHandler<T> = fn(&[Arg], &Session<T>) -> anyhow::Result<()>;
pub type CommandInContext<T> = fn(&Session<T>) -> bool;

pub struct ReplCommand<T: 'static> {
    pub cmd: String,
    pub op: Option<String>,
    pub hint: String,
    pub handler: CommandHandler<T>,
    /// Predicates checked against the session to decide whether this command applies to
    /// the current context. `None` means the command always applies. When present, the
    /// command applies if *all* predicate matches (AND semantics) - build this with the
    /// `in_context!` macro, e.g. `in_context!(feature_ctx, identity_ctx)`.
    pub has_context: Option<&'static [CommandInContext<T>]>,
}

/// A struct extending a simple string slice with the position
/// at which the string was found in the command line during argument parsing.
#[derive(Debug, PartialEq)]
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
    /// The array is expected to be composed of the following elements:
    ///
    /// [COMMAND, operation, arg, arg, ...]
    ///
    /// Matching succeeds only if the command and operation in the array match
    /// self's `cmd` and `op` respectively.
    pub fn matches_slices(&self, slices: &[&str]) -> bool {
        !slices.is_empty()
            && slices.first().unwrap().to_uppercase() == self.cmd
            && match (&self.op, slices.get(1)) {
                // Command has an op which matches the first provided argument
                (Some(op), Some(arg)) => op == arg,
                // Command has an op but none has been provided
                (Some(_), None) => false,
                // Command has no op - treat all elements as arguments
                (None, _) => true,
            }
    }

    /// Returns the remaining part of the hint for already entered command-line slices.
    /// Depending on how many slices have been provided, only a portion of
    /// the hint may be returned, e.g.:
    ///
    ///  - for "COMMAND op" - since no arguments to the "op" operation have been provided,
    ///    a hint describing all missing arguments is returned.
    ///  - for "COMMAND op arg" - since the first argument ("arg") has already been provided,
    ///    only the portion of the hint describing the second argument is returned.
    pub fn remaining_hint(&self, slices: &[&str]) -> &str {
        // Determines how many slices to skip initially. The command is always skipped;
        // the operation is skipped only when one is available.
        let slices_to_skip = match self.op {
            // Skip command and op
            Some(_) => 2,
            // No op, skip command only
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
