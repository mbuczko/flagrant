use argh::FromArgs;
use colored::Colorize;
use command::Command;
use completer::ArgCompleter;
use flagrant_client::{
    connection::Connection,
    http::{Auth, HttpClient},
};
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

#[derive(FromArgs)]
/// Flagrant feature flag CLI
struct Args {
    /// API host (default: http://localhost:3030)
    #[argh(
        option,
        short = 'h',
        default = "String::from(\"http://localhost:3030\")"
    )]
    host: String,

    /// project name (mutually exclusive with --create-project)
    #[argh(option, short = 'p')]
    project: Option<String>,

    /// environment ID
    #[argh(option, short = 'e', default = "1")]
    environment: i32,

    /// create a new project with this name and use it for the session (mutually exclusive with --project)
    #[argh(option)]
    create_project: Option<String>,

    /// list all projects
    #[argh(switch)]
    list_projects: bool,
}

fn prompter(session: &Session<Connection>) -> String {
    let ctx = session.context.read().unwrap();
    let dirty_feature = ctx
        .feature_patch
        .as_ref()
        .map(|p| !p.is_empty())
        .unwrap_or(false);
    let dirty_identity = ctx.has_identity_pending();
    let feat = match &ctx.feature {
        Some(feat) if dirty_feature => format!(" → {}*", feat.name),
        Some(feat) => format!(" → {}", feat.name),
        _ => String::default(),
    };
    let id = match &ctx.identity {
        Some(id) if dirty_identity => format!(" @ {}*", id.value),
        Some(id) => format!(" @ {}", id.value),
        _ => String::default(),
    };
    format!(
        "{}/{}{}{}\x1b[0m › ",
        ctx.project.name,
        ctx.environment.name.purple(),
        feat.green(),
        id.cyan()
    )
}

fn has_feature_ctx(session: &Session<Connection>) -> bool {
    session.context.read().unwrap().feature.is_some()
}
fn has_identity_ctx(session: &Session<Connection>) -> bool {
    session.context.read().unwrap().identity.is_some()
}
fn has_feature_and_identity_ctx(session: &Session<Connection>) -> bool {
    let ctx = session.context.read().unwrap();
    ctx.feature.is_some() && ctx.identity.is_some()
}
fn has_pending_ctx(session: &Session<Connection>) -> bool {
    let ctx = session.context.read().unwrap();
    (ctx.feature.is_some()
        && ctx
            .feature_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false))
        || (ctx.identity.is_some() && ctx.has_identity_pending())
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    if args.list_projects {
        let client = HttpClient::new(args.host.clone(), Auth::None);
        let projects = handlers::projects::list_projects(&client)?;
        println!("Known projects:\n---------------");
        for project in projects {
            println!("{}", project.name);
        }
        return Ok(());
    }

    let connection = match (args.project, args.create_project) {
        (Some(project_name), None) => {
            Connection::init(args.host, Auth::None, project_name, args.environment)?
        }
        (None, Some(name)) => {
            let client = HttpClient::new(args.host.clone(), Auth::None);
            let (project, env) = handlers::projects::create_with_env(&name, &client)?;
            Connection::init(args.host, Auth::None, project.name, env.id)?
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("--project and --create-project are mutually exclusive")
        }
        (None, None) => anyhow::bail!("one of --project or --create-project must be provided"),
    };

    let session = Session::new(connection);
    let commands = vec![
        // Environments
        Command::Environment.op("add", "environment base", handlers::environments::add),
        Command::Environment.op("use", "environment", handlers::environments::r#use),
        Command::Environment.op("list", "", handlers::environments::list),
        Command::Environment.args("add · list · use"),
        // Features
        Command::Feature.op(
            "list",
            "archived|enabled|tag|[pattern]",
            handlers::features::list,
        ),
        Command::Feature.op("add", "feature value", handlers::features::add),
        Command::Feature.op("describe", "feature", handlers::features::describe),
        Command::Feature.op("delete", "feature", handlers::features::delete),
        Command::Feature.op("use", "feature", handlers::features::r#use),
        Command::Feature.args("add · delete · describe · list · use"),
        // Identities
        Command::Identity.op(
            "add",
            "identity [trait:value ...]",
            handlers::identities::add,
        ),
        Command::Identity.op("list", "[pattern]", handlers::identities::list),
        Command::Identity.op("describe", "[identity]", handlers::identities::describe),
        Command::Identity.op("delete", "identity", handlers::identities::delete),
        Command::Identity.op("use", "identity", handlers::identities::r#use),
        Command::Identity.args("add · delete · describe · list · use"),
        // Variants
        Command::Variant.op_in_context("list", "", handlers::variants::list, has_feature_ctx),
        Command::Variant.op_in_context(
            "add",
            "weight value",
            handlers::variants::add,
            has_feature_ctx,
        ),
        Command::Variant.op_in_context(
            "delete",
            "index",
            handlers::variants::delete,
            has_feature_ctx,
        ),
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
            "index [+/-]weight",
            handlers::variants::weight,
            has_feature_ctx,
        ),
        Command::Variant.args_in_context(
            "list · add · delete · discard · weight · value",
            has_feature_ctx,
        ),
        // Feature setters (only in feature context)
        Command::Set.op_in_context(
            "status",
            "on|off|archived",
            handlers::features::set_status,
            has_feature_ctx,
        ),
        Command::Set.op_in_context(
            "value",
            "value",
            handlers::features::set_value,
            has_feature_ctx,
        ),
        // Identity setters (only in identity context)
        Command::Set.op_in_context(
            "trait",
            "name:value",
            handlers::identities::set_trait,
            has_identity_ctx,
        ),
        Command::Set.op_in_context(
            "override",
            "[value]",
            handlers::identities::set_override,
            has_feature_and_identity_ctx,
        ),
        Command::Set.args("status · value · override · trait · identity"),
        // UNSET (only in identity context)
        Command::Unset.op_in_context(
            "trait",
            "name",
            handlers::identities::unset_trait,
            has_identity_ctx,
        ),
        Command::Unset.args_in_context("trait", has_identity_ctx),
        // Commit / discard (available when any context has pending changes)
        Command::Commit.no_op_in_context(
            "→ commit staged changes",
            handlers::commit,
            has_pending_ctx,
        ),
        Command::Discard.no_op_in_context(
            "→ discard staged changes",
            handlers::discard,
            has_pending_ctx,
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
