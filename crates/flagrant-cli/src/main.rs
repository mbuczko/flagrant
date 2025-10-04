use command::Command;
use flagrant_client::{
    connection::{Connection, Resource},
    http::Auth,
};
use flagrant_repl::{
    completer::{AutoCompleter, CommandCompleter},
    hinter::ReplHinter,
    readline::{self, ReplHelper},
    session::Session,
};
use flagrant_types::{Environment, Feature, Tag};
use rustyline::overlay::GenericOverlayer;

mod command;
mod handlers;
mod printer;

const API_HOST: &str = "http://localhost:3030";

struct ArgCompleter<'a> {
    session: &'a Session<Connection>,
}

fn strip_tag(input: &str) -> (&str, Option<char>, &str) {
    let (lhs, rhs) = match input.rsplit_once(',') {
        Some((l, r)) => (l, r),
        _ => ("", input),
    };
    match rhs.char_indices().next() {
        Some((_, m)) if m == '-' => (lhs, Some(m), &rhs[1..]),
        _ => (lhs, None, rhs),
    }
}

// TODO(michal): move to separate module
impl AutoCompleter for ArgCompleter<'_> {
    fn complete_by_prefix(&self, command: &str, prefix: &str) -> anyhow::Result<Vec<String>> {
        if let Some((prop, val)) = prefix.split_once(':') {
            match prop {
                "tag" => {
                    let ctx = self.session.context.read().unwrap();
                    let res = ctx.environment.as_base_resource();
                    let (lhs, modifier, val) = strip_tag(val);
                    Ok(ctx
                        .client
                        .get::<Vec<Tag>>(res.subpath(format!("/tags?prefix={val}")))?
                        .into_iter()
                        .map(|c| {
                            let mut tag = String::with_capacity(c.name.len() + 2);
                            if !lhs.is_empty() {
                                tag.push(',');
                            }
                            if let Some(m) = modifier {
                                tag.push(m);
                            }
                            tag.push_str(&c.name);
                            format!("{prop}:{lhs}{tag}")
                        })
                        .collect::<Vec<_>>())
                }
                _ => Ok(vec![]),
            }
        } else {
            match command.to_uppercase().as_ref() {
                "ENVIRONMENT" => {
                    let ctx = self.session.context.read().unwrap();
                    let res = ctx.project.as_base_resource();

                    Ok(ctx
                        .client
                        .get::<Vec<Environment>>(res.subpath(format!("/envs?prefix={prefix}")))?
                        .into_iter()
                        .map(|c| c.name)
                        .collect::<Vec<_>>())
                }

                // auto-complete feature name both for "feature" and "variant" commands
                "FEATURE" | "VARIANT" => {
                    let ctx = self.session.context.read().unwrap();
                    let res = ctx.environment.as_base_resource();

                    Ok(ctx
                        .client
                        .get::<Vec<Feature>>(res.subpath(format!("/features?prefix={prefix}")))?
                        .into_iter()
                        .map(|c| c.name)
                        .collect::<Vec<_>>())
                }
                _ => Ok(vec![]),
            }
        }
    }
}

fn prompter(session: &Session<Connection>) -> String {
    let ctx = session.context.read().unwrap();
    format!(
        "[{}/\x1b[35m{}\x1b[0m] > ",
        ctx.project.name, ctx.environment.name
    )
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
        Command::Environment.op("set", "environment", handlers::env::switch),
        Command::Environment.op("list", "", handlers::env::list),
        Command::Environment.args("add | list | set"),
        // features
        Command::Feature.op("list", "", handlers::feat::list),
        Command::Feature.op("add", "feature value", handlers::feat::add),
        Command::Feature.op("set", "feature", handlers::feat::switch),
        Command::Feature.op("delete", "feature", handlers::feat::delete),
        Command::Feature.op("value", "feature value", handlers::feat::value),
        Command::Feature.op("on", "feature", handlers::feat::on),
        Command::Feature.op("off", "feature", handlers::feat::off),
        Command::Feature.args("add | delete | list | on | off | value"),
        // variants
        Command::Variant.op("list", "feature", handlers::var::list),
        Command::Variant.op("add", "feature weight value", handlers::var::add),
        Command::Variant.op("delete", "variant-id", handlers::var::del),
        Command::Variant.op("value", "variant-id value", handlers::var::value),
        Command::Variant.op("weight", "variant-id weight", handlers::var::weight),
        Command::Variant.args("add | delete | list | value | weight"),
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
        completer: CommandCompleter::new(
            commands
                .iter()
                .map(|c| (c.cmd.to_uppercase(), &c.op))
                .collect::<Vec<_>>(),
        )
        .with_arg_completer(&arg_completer),
    };

    readline::init(helper, &session, &commands)?;

    Ok(())
}
