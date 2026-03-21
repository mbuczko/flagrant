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
    let dirty = ctx.pending.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
    let feat = match &ctx.feature {
        Some(feat) if dirty => format!(" → {}*", feat.name),
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
fn has_feature_with_pending_ctx(session: &Session<Connection>) -> bool {
    let ctx = &session.context.read().unwrap();
    ctx.feature.is_some() && ctx.pending.is_some()
}

fn main() -> anyhow::Result<()> {
    // TODO: will be taken from args
    let project_id = 1;
    let environment_id = 1;

    let connection = Connection::init(API_HOST.into(), Auth::None, project_id, environment_id)?;
    let session = Session::new(connection);
    let commands = vec![
        // Environments
        Command::Environment.op(
            "add",
            "environment description",
            handlers::environments::add,
        ),
        Command::Environment.op("use", "environment", handlers::environments::r#use),
        Command::Environment.op("list", "", handlers::environments::list),
        Command::Environment.args("add · list · use"),
        // Features
        Command::Feature.op("list", "filter", handlers::features::list),
        Command::Feature.op("add", "feature value", handlers::features::add),
        Command::Feature.op("describe", "feature", handlers::features::describe),
        Command::Feature.op("delete", "feature", handlers::features::delete),
        Command::Feature.op("use", "feature", handlers::features::r#use),
        Command::Feature.args("add · delete · describe · list · use"),
        // Variants
        Command::Variant.op_in_context("list", "", handlers::variants::list, has_feature_ctx),
        Command::Variant.op_in_context(
            "add",
            "weight value",
            handlers::variants::add,
            has_feature_ctx,
        ),
        Command::Variant.op_in_context("delete", "index", handlers::variants::delete, has_feature_ctx),
        Command::Variant.op_in_context(
            "discard",
            "index",
            handlers::variants::discard,
            has_feature_ctx,
        ),
        Command::Variant.op_in_context(
            "value",
            "index value",
            handlers::variants::value,
            has_feature_ctx,
        ),
        Command::Variant.op_in_context(
            "weight",
            "index weight",
            handlers::variants::weight,
            has_feature_ctx,
        ),
        Command::Variant.args_in_context(
            "list · add · delete · discard · weight · value",
            has_feature_ctx,
        ),
        // Feature setters (only available in feature context)
        Command::Set.op_in_context(
            "state",
            "on|off",
            handlers::features::state,
            has_feature_ctx,
        ),
        Command::Set.op_in_context(
            "status",
            "active|inactive",
            handlers::features::status,
            has_feature_ctx,
        ),
        Command::Set.op_in_context(
            "value",
            "value",
            handlers::features::set_value,
            has_feature_ctx,
        ),
        Command::Set.args_in_context("state · status · value", has_feature_ctx),
        // Commit / discard (only available in feature context)
        Command::Commit.no_op_in_context(
            "→ commit staged changes",
            handlers::features::commit,
            has_feature_with_pending_ctx,
        ),
        Command::Discard.no_op_in_context(
            "→ discard staged changes",
            handlers::features::discard,
            has_feature_with_pending_ctx,
        ),
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
