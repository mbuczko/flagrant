use colored::Colorize;
use command::Command;
use completer::ArgCompleter;
use flagrant_client::{connection::Connection, http::Auth};
use flagrant_repl::{
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

fn has_feature_ctx(session: &Session<Connection>) -> bool {
    let ctx = &session.context.read().unwrap();
    ctx.feature.is_some()
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
        Command::Environment.op("use", "environment", handlers::env::r#use),
        Command::Environment.op("list", "", handlers::env::list),
        Command::Environment.args("add · list · use"),
        // features
        Command::Feature.op("list", "filter", handlers::feat::list),
        Command::Feature.op("add", "feature value", handlers::feat::add),
        Command::Feature.op("delete", "feature", handlers::feat::delete),
        Command::Feature.op("use", "feature", handlers::feat::r#use),
        Command::Feature.args("add · delete · list · use"),
        // variants
        Command::Variant.op_in_context("list", "", handlers::var::list, has_feature_ctx),
        Command::Variant.op_in_context("add", "weight value", handlers::var::add, has_feature_ctx),
        Command::Variant.op_in_context("delete", "variant-id", handlers::var::del, has_feature_ctx),
        Command::Variant.op_in_context(
            "value",
            "variant-id value",
            handlers::var::value,
            has_feature_ctx,
        ),
        Command::Variant.op_in_context(
            "weight",
            "variant-id weight",
            handlers::var::weight,
            has_feature_ctx,
        ),
        Command::Variant.args_in_context("list · add · delete · value", has_feature_ctx),
        // feature setters (only available in feature context)
        Command::Set.op_in_context("on", "", handlers::feat::on, has_feature_ctx),
        Command::Set.op_in_context("off", "", handlers::feat::off, has_feature_ctx),
        Command::Set.op_in_context("value", "value", handlers::feat::value, has_feature_ctx),
        Command::Set.op_in_context("value", "value", handlers::feat::value, has_feature_ctx),
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
        completer: CommandLineCompleter::new({
            let session_ref = &session;
            commands
                .iter()
                .map(|c| {
                    // Convert function pointer fn(&Session) -> bool to closure Fn() -> bool
                    // by capturing session_ref. This allows the completer to check context
                    // without needing direct access to the session.
                    let context_checker = c.has_context.map(|checker| {
                        Box::new(move || checker(session_ref)) as Box<dyn Fn() -> bool>
                    });
                    (c.cmd.to_uppercase(), &c.op, context_checker)
                })
                .collect()
        })
        .with_arg_completer(&arg_completer),
    };

    readline::init(helper, &session, &commands)?;

    Ok(())
}
