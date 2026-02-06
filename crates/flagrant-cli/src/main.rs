use colored::Colorize;
use command::Command;
use completer::ArgCompleter;
use flagrant_client::{connection::Connection, http::Auth};
use flagrant_repl::{
    command::ReplCommand,
    completer::CommandLineCompleter,
    hinter::ReplHinter,
    readline::{self, ReplHelper},
    session::Session,
};
use rustyline::overlay::GenericOverlayer;

mod command;
mod completer;
mod handlers;
mod printer;

const API_HOST: &str = "http://localhost:3030";

fn prompter(session: &Session<Connection>) -> String {
    let ctx = session.context.read().unwrap();
    let feat = match &ctx.feature {
        Some(feat) => format!(" → {}", feat.name),
        _ => String::default(),
    };
    format!(
        "{}/{}{}\x1b[0m › ",
        ctx.project.name,
        ctx.environment.name.purple(),
        feat.green()
    )
}
fn stringify_commands(commands: &[ReplCommand<Connection>]) -> Vec<(String, &Option<String>)> {
    commands
        .iter()
        .map(|c| (c.cmd.to_uppercase(), &c.op))
        .collect::<Vec<_>>()
}

fn main() -> anyhow::Result<()> {
    // todo: will be taken from args
    let project_id = 1;
    let environment_id = 1;

    let connection = Connection::init(API_HOST.into(), Auth::None, project_id, environment_id)?;
    let session = Session::new(connection);

    let commands = vec![
        // environments
        Command::Environment.op("add", "environment description", handlers::env::add),
        Command::Environment.op("set", "environment", handlers::env::set),
        Command::Environment.op("list", "", handlers::env::list),
        Command::Environment.args("add · list · set"),
        // features
        Command::Feature.op("list", "filter", handlers::feat::list),
        Command::Feature.op("add", "feature value", handlers::feat::add),
        Command::Feature.op("delete", "feature", handlers::feat::delete),
        Command::Feature.op("set", "feature", handlers::feat::set),
        Command::Feature.args("add · delete · list · set"),
        // variants
        Command::Variant.op("list", "feature", handlers::var::list),
        Command::Variant.op("add", "feature weight value", handlers::var::add),
        Command::Variant.op("delete", "variant-id", handlers::var::del),
        Command::Variant.op("value", "variant-id value", handlers::var::value),
        Command::Variant.op("weight", "variant-id weight", handlers::var::weight),
        Command::Variant.args("add · delete · list · value · weight"),
    ];
    let feature_setters = vec![
        Command::Set.op("on", "", handlers::feat::on),
        Command::Set.op("off", "", handlers::feat::off),
        Command::Set.op("value", "value", handlers::feat::value),
    ];
    let overlays = vec![
        (']', "\x1b[36mdir> \x1b[0m"),
        ('?', "\x1b[33mhelp> \x1b[0m"),
        ('\\', "\x1b[36mset> \x1b[0m"),
    ];
    let arg_completer = ArgCompleter { session: &session };
    let helper = ReplHelper {
        prompter,
        hinter: ReplHinter::new(&commands),
        overlayer: GenericOverlayer { pairs: overlays },
        completer: CommandLineCompleter::new(stringify_commands(&commands), || {
            let ctx = &session.context.read().unwrap();
            if ctx.feature.is_some() {
                return Some(stringify_commands(&feature_setters));
            }
            None
        })
        .with_arg_completer(&arg_completer),
    };

    readline::init(helper, &session, &commands)?;

    Ok(())
}
