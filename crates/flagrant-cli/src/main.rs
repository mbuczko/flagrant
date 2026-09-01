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
use flagrant_types::Project;
use rustyline::overlay::GenericOverlayer;

mod command;
mod completer;
mod handlers;
mod help;
mod printer;

/// First-character trigger for the `?` help overlay - shared by the cosmetic prompt
/// overlay, the reduced tab-completion mode, and the REPL's help dispatch, so they
/// can't drift out of sync.
const HELP_TRIGGER: char = '?';

/// First-character shortcut for switching environments - `/dev` is rewritten into
/// `USE /dev` before completion and dispatch, so it behaves exactly like typing the
/// full `USE /dev` command (live environment-name completion, same switch logic).
const ENV_TRIGGER: char = '/';

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

    /// project name - opens it if it exists, or creates and opens it if it doesn't
    #[argh(option, short = 'p')]
    project: Option<String>,

    /// environment ID (default: the project's first-created environment)
    #[argh(option, short = 'e')]
    environment: Option<i32>,

    /// list all projects
    #[argh(switch)]
    list_projects: bool,
}

fn print_banner() {
    println!(
        "\n  {} {}{}",
        "Flagrant".bold(),
        "⚡".yellow(),
        "CLI-driven feature flagging".dimmed()
    );
    println!(
        "\n  {} for environment switch",
        ENV_TRIGGER.to_string().cyan()
    );
    println!("  {} for help", HELP_TRIGGER.to_string().yellow());
    println!("\n  Use ⌫ to escape help/environment prompt.\n");
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
        Some(s) if dirty_segment => format!(" + {}*", s.name),
        Some(s) => format!(" + {}", s.name),
        None => String::default(),
    };
    format!(
        "{}/{}{}{}{}\x1b[0m › ",
        ctx.project.name,
        ctx.environment.name.bold(),
        feat.green(),
        id.green(),
        seg.green(),
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

    let Some(project_name) = args.project else {
        anyhow::bail!("--project must be provided");
    };
    let client = HttpClient::new(args.host.clone(), Auth::None);
    let connection = match client.get::<Project>(format!("/projects/{project_name}")) {
        Ok(_) => Connection::init(args.host, Auth::None, project_name, args.environment)?,
        Err(_) => {
            let (project, env) = handlers::projects::create_with_env(&project_name, &client)?;
            Connection::init(args.host, Auth::None, project.name, Some(env.id))?
        }
    };

    print_banner();

    let session = Session::new(connection);
    let commands = vec![
        // Environments
        Command::Environment.op("add", "environment base", handlers::environments::add),
        Command::Environment.op("list", "", handlers::environments::list),
        Command::Environment.op("show", "[name]", handlers::environments::show),
        Command::Environment.op(
            "describe",
            "[description]",
            handlers::environments::describe,
        ),
        Command::Environment.args("add · describe · list · show"),
        // Features
        Command::Feature.op("list", "status|tag|[pattern]", handlers::features::list),
        Command::Feature.op("add", "feature value", handlers::features::add),
        Command::Feature.op("show", "feature", handlers::features::show),
        Command::Feature.op_in_context(
            "rename",
            "[name]",
            handlers::features::rename,
            in_context!(feature_ctx),
        ),
        Command::Feature.op_in_context(
            "describe",
            "[description]",
            handlers::features::describe,
            in_context!(feature_ctx),
        ),
        Command::Feature.op_in_context(
            "status",
            "on|off|archived",
            handlers::features::status,
            in_context!(feature_ctx),
        ),
        Command::Feature.op_in_context(
            "server-side",
            "on|off",
            handlers::features::server_side,
            in_context!(feature_ctx),
        ),
        Command::Feature.op_in_context(
            "tag",
            "[tag1 [tag2 ...]]",
            handlers::features::tag,
            in_context!(feature_ctx),
        ),
        Command::Feature.op_in_context(
            "progressive",
            "rules <w1>:<dur1> ... <100> | sample <n> | delete | status",
            handlers::features::progressive,
            in_context!(feature_ctx),
        ),
        Command::Feature.op("delete", "feature", handlers::features::delete),
        // Context-gated hint must come before the unconditional one below - `find()` takes
        // the first match, and the unconditional entry (op: None) would otherwise shadow
        // any real op registered after it.
        Command::Feature.args_in_context(
            "add · delete · describe · list · progressive · rename · show · server-side · status · tag",
            in_context!(feature_ctx),
        ),
        Command::Feature.args("add · delete · list · show"),
        // Identities
        Command::Identity.op(
            "add",
            "identity [trait=value ...]",
            handlers::identities::add,
        ),
        Command::Identity.op("list", "trait|[pattern]", handlers::identities::list),
        Command::Identity.op("show", "[identity]", handlers::identities::show),
        Command::Identity.op("delete", "identity", handlers::identities::delete),
        Command::Identity.op("drop!", "pattern", handlers::identities::drop_matching),
        Command::Identity.op_in_context(
            "trait",
            "name=value|-name [...]",
            handlers::identities::r#trait,
            in_context!(identity_ctx),
        ),
        // Context-gated hint must come before the unconditional one below - see the
        // Feature block above for why.
        Command::Identity.args_in_context(
            "add · delete · drop! · list · show · trait",
            in_context!(identity_ctx),
        ),
        Command::Identity.args("add · delete · drop! · list · show"),
        // Variants
        Command::Variant.op_in_context(
            "add",
            "weight value",
            handlers::variants::add,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "delete",
            "[index]",
            handlers::variants::delete,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "show",
            "[index]",
            handlers::variants::show,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "value",
            "[index] [value]",
            handlers::variants::value,
            in_context!(feature_ctx),
        ),
        Command::Variant.op_in_context(
            "weight",
            "[index [+/-]weight]",
            handlers::variants::weight,
            in_context!(feature_ctx),
        ),
        Command::Variant.args_in_context(
            "add · delete · show · weight · value",
            in_context!(feature_ctx),
        ),
        // Segments
        Command::Segment.op("add", "name [description]", handlers::segments::add),
        Command::Segment.op("list", "[pattern]", handlers::segments::list),
        Command::Segment.op("show", "[name]", handlers::segments::show),
        Command::Segment.op_in_context(
            "describe",
            "[description]",
            handlers::segments::describe,
            in_context!(segment_ctx),
        ),
        Command::Segment.op("delete", "[name]", handlers::segments::delete),
        Command::Segment.op_in_context(
            "rename",
            "[name]",
            handlers::segments::rename,
            in_context!(segment_ctx),
        ),
        // Context-gated hint must come before the unconditional one below - see the
        // Feature block above for why.
        Command::Segment.args_in_context(
            "add · delete · describe · list · rename · show",
            in_context!(segment_ctx),
        ),
        Command::Segment.args("add · delete · list · show"),
        // Groups (only in segment context)
        Command::Group.op_in_context(
            "add",
            "[--and|--and-not] [description]",
            handlers::groups::add,
            in_context!(segment_ctx),
        ),
        Command::Group.op_in_context(
            "show",
            "[label]",
            handlers::groups::show,
            in_context!(segment_ctx),
        ),
        Command::Group.op_in_context(
            "describe",
            "[label] [description]",
            handlers::groups::describe,
            in_context!(segment_ctx),
        ),
        Command::Group.op_in_context(
            "delete",
            "[label]",
            handlers::groups::delete,
            in_context!(segment_ctx),
        ),
        Command::Group.args_in_context("add · delete · describe · show", in_context!(segment_ctx)),
        // Rules (only in segment context)
        Command::Rule.op_in_context(
            "add",
            "group-label <identity|trait|environment> comparator value",
            handlers::rules::add,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "show",
            "[group-label [rule-index]]",
            handlers::rules::show,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "delete",
            "[group-label [rule-index]]",
            handlers::rules::delete,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "value",
            "[group-label [rule-index]] [value]",
            handlers::rules::value,
            in_context!(segment_ctx),
        ),
        Command::Rule.op_in_context(
            "comparator",
            "[group-label [rule-index]] [comparator]",
            handlers::rules::comparator,
            in_context!(segment_ctx),
        ),
        Command::Rule.args_in_context(
            "add · delete · show · value · comparator",
            in_context!(segment_ctx),
        ),
        // Snapshots (only in feature context)
        Command::Snapshot.op_in_context(
            "list",
            "",
            handlers::snapshots::list,
            in_context!(feature_ctx),
        ),
        Command::Snapshot.op_in_context(
            "show",
            "version",
            handlers::snapshots::show,
            in_context!(feature_ctx),
        ),
        Command::Snapshot.op_in_context(
            "describe",
            "[version] [comment]",
            handlers::snapshots::describe,
            in_context!(feature_ctx),
        ),
        Command::Snapshot.op_in_context(
            "restore",
            "version [comment]",
            handlers::snapshots::restore,
            in_context!(feature_ctx),
        ),
        Command::Snapshot.args_in_context(
            "describe · list · restore · show",
            in_context!(feature_ctx),
        ),
        // Commit / discard (available when any context has pending changes)
        Command::Commit.no_op_in_context("[comment]", handlers::commit, in_context!(pending_ctx)),
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
        Command::Reload.no_op("→ reload server configuration", handlers::admin::reload),
        // Unified context switch, replacing the old FEATURE/IDENTITY/SEGMENT/ENVIRONMENT
        // use ops: a bare name is a feature, `@name` an identity, `+name` a segment,
        // `/name` an environment, and `feature@identity` / `feature+segment` combine a
        // feature switch with one.
        Command::Use.no_op(
            "feature | @identity | +segment | /environment",
            handlers::context::r#use,
        ),
        // Query resolved feature values for an identity, without mutating any context
        Command::Get.no_op("[feature][@identity]", handlers::tester::get),
        Command::GetAll.no_op("[@identity]", handlers::tester::get_all),
        // Identity overrides (only in identity context)
        Command::Override.op_in_context(
            "add",
            "[variant-index]",
            handlers::identities::set_override,
            in_context!(identity_ctx),
        ),
        Command::Override.op_in_context(
            "delete",
            "",
            handlers::identities::unset_override,
            in_context!(identity_ctx),
        ),
        Command::Override.args_in_context("add · delete", in_context!(identity_ctx)),
        // Segment overrides (only in feature + segment context)
        Command::Override.op_in_context(
            "add",
            "[variant-index weight]",
            handlers::segments::set_override,
            in_context!(feature_ctx, segment_ctx),
        ),
        Command::Override.op_in_context(
            "delete",
            "",
            handlers::segments::unset_override,
            in_context!(feature_ctx, segment_ctx),
        ),
        Command::Override.args_in_context("add · delete", in_context!(feature_ctx, segment_ctx)),
        // UNSET (only in feature context)
        Command::Unset.op_in_context(
            "distribution",
            "pattern",
            handlers::features::unset_distribution,
            in_context!(feature_ctx),
        ),
        Command::Unset.args_in_context("distribution", in_context!(feature_ctx)),
    ];
    let overlays = vec![
        (HELP_TRIGGER, "\x1b[33mhelp> \x1b[0m"),
        (ENV_TRIGGER, "\x1b[36menvironment> \x1b[0m"),
    ];
    let help_topics: Vec<String> = help::TOPICS.iter().map(|s| s.to_string()).collect();
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
        .with_arg_completer(&arg_completer)
        .with_help_topics(HELP_TRIGGER, help_topics)
        .with_shortcut(ENV_TRIGGER, "USE"),
    };

    readline::init(
        helper,
        &session,
        &commands,
        Some((HELP_TRIGGER, help::show)),
        Some((ENV_TRIGGER, "USE")),
    )?;

    Ok(())
}
