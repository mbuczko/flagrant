use flagrant_client::connection::Connection;
use flagrant_repl::{
    command::{Arg, CommandHandler, CommandInContext, ReplCommand},
    session::Session,
};
use strum_macros::{Display, EnumIter, EnumString};

/// Predicates checked against the session to decide whether a command applies to the
/// current context; `None` means the command always applies, `Some` requires every
/// predicate to match (AND semantics) - see `in_context!`.
type ContextPredicates = Option<&'static [CommandInContext<Connection>]>;

/// Builds a `&'static [fn(&Session<Connection>) -> bool]` from one or more context
/// predicates, to be passed to `op_in_context` / `args_in_context` / `no_op_in_context`.
/// The command applies if *all* predicates match (AND semantics), e.g.
/// `in_context!(feature_ctx, identity_ctx)` applies only when both feature AND identity
/// context are active, without needing a dedicated hand-written composite predicate
/// function.
#[macro_export]
macro_rules! in_context {
    ($($pred:expr),+ $(,)?) => {
        &[$($pred),+]
    };
}

#[derive(Debug, Display, EnumIter, EnumString)]
pub enum Command {
    Environment,
    Feature,
    Identity,
    Variant,
    Segment,
    Group,
    Rule,
    Set,
    Unset,
    Commit,
    Discard,
    Reset,
}

impl Command {
    /// No-op command handler used to ignore commands called with no required arguments.
    fn no_op_handler(_args: &[Arg], _session: &Session<Connection>) -> anyhow::Result<()> {
        Ok(())
    }

    /// Generic command builder.
    /// Creates a new command with or without an operation and command handler function.
    fn build(
        &self,
        op: Option<&str>,
        hint: &str,
        handler: Option<CommandHandler<Connection>>,
        has_context: ContextPredicates,
    ) -> ReplCommand<Connection> {
        ReplCommand {
            cmd: self.to_string().to_uppercase(),
            op: op.map(String::from),
            hint: hint.to_owned(),
            handler: handler.unwrap_or(Self::no_op_handler),
            has_context,
        }
    }

    /// Builds a command handler for provided operation
    pub fn op(
        &self,
        op: &str,
        hint: &str,
        handler: CommandHandler<Connection>,
    ) -> ReplCommand<Connection> {
        self.build(Some(op), hint, Some(handler), None)
    }

    pub fn op_in_context(
        &self,
        op: &str,
        hint: &str,
        handler: CommandHandler<Connection>,
        has_context: &'static [CommandInContext<Connection>],
    ) -> ReplCommand<Connection> {
        self.build(Some(op), hint, Some(handler), Some(has_context))
    }

    /// Builds a no-op (no-operation) version of command.
    #[allow(dead_code)]
    pub fn no_op(
        &self,
        hint: &str,
        handler: CommandHandler<Connection>,
    ) -> ReplCommand<Connection> {
        self.build(None, hint, Some(handler), None)
    }

    #[allow(dead_code)]
    pub fn no_op_in_context(
        &self,
        hint: &str,
        handler: CommandHandler<Connection>,
        has_context: &'static [CommandInContext<Connection>],
    ) -> ReplCommand<Connection> {
        self.build(None, hint, Some(handler), Some(has_context))
    }

    /// When invoked, command will be handled by `no_op_handler` which does nothing.
    pub fn args(&self, hint: &str) -> ReplCommand<Connection> {
        self.build(None, hint, None, None)
    }

    pub fn args_in_context(
        &self,
        hint: &str,
        has_context: &'static [CommandInContext<Connection>],
    ) -> ReplCommand<Connection> {
        self.build(None, hint, None, Some(has_context))
    }
}
