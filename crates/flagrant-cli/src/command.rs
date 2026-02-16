use flagrant_client::connection::Connection;
use flagrant_repl::{
    command::{Arg, CommandHandler, ReplCommand},
    session::Session,
};
use strum_macros::{Display, EnumIter, EnumString};

#[derive(Debug, Display, EnumIter, EnumString)]
pub enum Command {
    Environment,
    Feature,
    Variant,
    Set,
}

impl Command {
    /// No-op command handler used to ignore commands called with no required arguments.
    fn no_op_handler(_args: &[Arg], _session: &Session<Connection>) -> anyhow::Result<()> {
        Ok(())
    }

    /// Generic command builder.
    /// Creates a new Command with or without operation and command handler function.
    fn build(
        &self,
        op: Option<&str>,
        hint: &str,
        handler: Option<CommandHandler<Connection>>,
        has_context: Option<fn(&Session<Connection>) -> bool>,
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
        has_context: fn(&Session<Connection>) -> bool,
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

    /// When invoked, command will be handled by `no_op_handler` which does nothing.
    pub fn args(&self, hint: &str) -> ReplCommand<Connection> {
        self.build(None, hint, None, None)
    }

    pub fn args_in_context(
        &self,
        hint: &str,
        has_context: fn(&Session<Connection>) -> bool,
    ) -> ReplCommand<Connection> {
        self.build(None, hint, None, Some(has_context))
    }
}
