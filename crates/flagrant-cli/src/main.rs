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
    let dirty_segment = ctx.has_segment_pending();
    let seg = match &ctx.segment {
        Some(s) if dirty_segment => format!(" [{}*]", s.name),
        Some(s) => format!(" [{}]", s.name),
        None => String::default(),
    };
    format!(
        "{}/{}{}{}{}\x1b[0m › ",
        ctx.project.name,
        ctx.environment.name.purple(),
        feat.green(),
        id.cyan(),
        seg.yellow()
    )
}

fn feature_ctx(session: &Session<Connection>) -> bool {
    session.context.read().unwrap().feature.is_some()
}
fn identity_ctx(session: &Session<Connection>) -> bool {
    session.context.read().unwrap().identity.is_some()
}
fn segment_ctx(session: &Session<Connection>) -> bool {
    session.context.read().unwrap().segment.is_some()
}
fn any_ctx(session: &Session<Connection>) -> bool {
    let ctx = session.context.read().unwrap();
    ctx.feature.is_some() || ctx.identity.is_some() || ctx.segment.is_some()
}
fn pending_ctx(session: &Session<Connection>) -> bool {
    let ctx = session.context.read().unwrap();
    (ctx.feature.is_some()
        && ctx
            .feature_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false))
        || (ctx.identity.is_some() && ctx.has_identity_pending())
        || (ctx.segment.is_some() && ctx.has_segment_pending())
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
        Command::Identity.op("list", "trait|[pattern]", handlers::identities::list),
        Command::Identity.op("describe", "[identity]", handlers::identities::describe),
        Command::Identity.op("delete", "pattern", handlers::identities::delete),
        Command::Identity.op("use", "identity", handlers::identities::r#use),
        Command::Identity.args("add · delete · describe · list · use"),
        // Variants
        Command::Variant.op_in_context(
            "add",
            "weight value",
            handlers::variants::add,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "delete",
            "index",
            handlers::variants::delete,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "discard",
            "index",
            handlers::variants::discard,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "value",
            "index value",
            handlers::variants::value,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "weight",
            "index [+/-]weight",
            handlers::variants::weight,
            in_context!(feature_ctx),
        ),
        Command::Variant.args_in_context(
            "add · delete · discard · weight · value",
            in_context!(feature_ctx),
        ),
        // Feature setters (only in feature context)
        Command::Set.op_in_context(
            "status",
            "on|off|archived",
            handlers::features::set_status,
            in_context!(feature_ctx),
        ),
        Command::Set.op_in_context(
            "value",
            "value",
            handlers::features::set_value,
            in_context!(feature_ctx),
        ),
        Command::Set.op_in_context(
            "description",
            "[description]",
            handlers::features::set_description,
            in_context!(feature_ctx),
        ),
        Command::Set.op_in_context(
            "tags",
            "tag1[, tag2, ...]",
            handlers::features::set_tags,
            in_context!(feature_ctx),
        ),
        // Identity setters (only in identity context)
        Command::Set.op_in_context(
            "trait",
            "name=value [name=value ...]",
            handlers::identities::set_trait,
            in_context!(identity_ctx),
        ),
        Command::Set.op_in_context(
            "override",
            "[value]",
            handlers::identities::set_override,
            in_context!(identity_ctx),
        ),
        // Segment setters (only in segment context)
        Command::Set.op_in_context(
            "name",
            "value",
            handlers::segments::set_name,
            in_context!(segment_ctx),
        ),
        Command::Set.op_in_context(
            "description",
            "value",
            handlers::segments::set_description,
            in_context!(segment_ctx),
        ),
        // Segment override (only when both feature and segment are in context)
        Command::Set.op_in_context(
            "override",
            "[variant-index weight]",
            handlers::segments::set_override,
            in_context!(feature_ctx, segment_ctx),
        ),
        Command::Set.args_in_context(
            "status · value · description · tags · name · override",
            in_context!(feature_ctx, segment_ctx),
        ),
        Command::Set.args_in_context(
            "status · value · description · tags · trait · override",
            in_context!(feature_ctx, identity_ctx),
        ),
        Command::Set.args_in_context(
            "status · value · description · tags",
            in_context!(feature_ctx),
        ),
        Command::Set.args_in_context("trait", in_context!(identity_ctx)),
        Command::Set.args_in_context("name · description", in_context!(segment_ctx)),
        // Feature unsetters (only in feature context)
        Command::Unset.op_in_context(
            "distribution",
            "pattern",
            handlers::features::unset_distribution,
            in_context!(feature_ctx),
        ),
        Command::Unset.op_in_context(
            "tags",
            "tag1[, tag2, ...]",
            handlers::features::unset_tags,
            in_context!(feature_ctx),
        ),
        // UNSET (only in identity context)
        Command::Unset.op_in_context(
            "trait",
            "name",
            handlers::identities::unset_trait,
            in_context!(identity_ctx),
        ),
        Command::Unset.op_in_context(
            "override",
            "",
            handlers::identities::unset_override,
            in_context!(identity_ctx),
        ),
        Command::Unset.op_in_context(
            "override",
            "",
            handlers::segments::unset_override,
            in_context!(segment_ctx),
        ),
        Command::Unset.args_in_context(
            "distribution · tags · override",
            in_context!(feature_ctx, segment_ctx),
        ),
        Command::Unset.args_in_context(
            "distribution · tags · trait · override",
            in_context!(feature_ctx, identity_ctx),
        ),
        Command::Unset.args_in_context("trait", in_context!(identity_ctx)),
        Command::Unset.args_in_context("distribution · tags", in_context!(feature_ctx)),
        // Segments
        Command::Segment.op("add", "name [description]", handlers::segments::add),
        Command::Segment.op("list", "", handlers::segments::list),
        Command::Segment.op("describe", "[name]", handlers::segments::describe),
        Command::Segment.op("delete", "name", handlers::segments::delete),
        Command::Segment.op("use", "name", handlers::segments::r#use),
        Command::Segment.args("add · delete · describe · list · use"),
        // Groups (only in segment context)
        Command::Group.op_in_context(
            "add",
            "[--and|--and-not] [description]",
            handlers::groups::add,
            in_context!(segment_ctx),
        ),
        Command::Group.op_in_context("list", "", handlers::groups::list, in_context!(segment_ctx)),
        Command::Group.op_in_context(
            "describe",
            "label",
            handlers::groups::describe,
            in_context!(segment_ctx),
        ),
        Command::Group.op_in_context(
            "delete",
            "label",
            handlers::groups::delete,
            in_context!(segment_ctx),
        ),
        Command::Group.args_in_context("add · list · describe · delete", in_context!(segment_ctx)),
        // Rules (only in segment context)
        Command::Rule.op_in_context(
            "add",
            "group-label <identity|trait|environment> comparator value",
            handlers::rules::add,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "describe",
            "group-label rule-index",
            handlers::rules::describe,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "delete",
            "group-label rule-index",
            handlers::rules::delete,
            in_context!(segment_ctx),
        ),
        Command::Rule.args_in_context("add · describe · delete", in_context!(segment_ctx)),
        // Commit / discard (available when any context has pending changes)
        Command::Commit.no_op_in_context(
            "→ commit staged changes",
            handlers::commit,
            in_context!(pending_ctx),
        ),
        Command::Discard.no_op_in_context(
            "→ discard staged changes",
            handlers::discard,
            in_context!(pending_ctx),
        ),
        Command::Reset.no_op_in_context(
            "→ reset feature and identity context",
            handlers::reset,
            in_context!(any_ctx),
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
        hinter: ReplHinter::new(&commands, &session),
        overlayer: GenericOverlayer { pairs: overlays },
        completer: CommandLineCompleter::new({
            let session_ref = &session;
            commands
                .iter()
                .map(|c| {
                    // Convert the slice of context predicates (AND-ed) into a closure
                    // Fn() -> bool by capturing session_ref. This allows the completer to
                    // check context without needing direct access to the session.
                    let context_checker = c.has_context.map(|checkers| {
                        Box::new(move || checkers.iter().all(|checker| checker(session_ref)))
                            as Box<dyn Fn() -> bool>
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
