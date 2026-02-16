use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, completer::AutoCompleter, session::Session};
use flagrant_types::{Environment, Feature, Tag};

pub struct ArgCompleter<'a> {
    pub session: &'a Session<Connection>,
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

impl AutoCompleter for ArgCompleter<'_> {
    fn complete_by_prefix(
        &self,
        command: &str,
        args: &[Arg],
        arg_n: usize,
        prefix: &str,
    ) -> anyhow::Result<Vec<String>> {
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

            "FEATURE" | "VARIANT" if arg_n >= 2 => {
                let ctx = self.session.context.read().unwrap();
                let res = ctx.environment.as_base_resource();
                let op: &str = &args[1];

                Ok(match op {
                    // Auto-complete feature name
                    "delete" | "use" if arg_n == 2 => ctx
                        .client
                        .get::<Vec<Feature>>(res.subpath(format!("/features?prefix={prefix}")))?
                        .into_iter()
                        .map(|c| c.name)
                        .collect::<Vec<_>>(),

                    // Auto-complete feature attributes names like tags or status
                    // along with attribute value (if completable)
                    "list" => match prefix.split_once(':') {
                        Some(("tag", val)) => {
                            let ctx = self.session.context.read().unwrap();
                            let res = ctx.environment.as_base_resource();
                            let (lhs, modifier, val) = strip_tag(val);

                            ctx.client
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
                                    format!("tag:{lhs}{tag}")
                                })
                                .collect::<Vec<_>>()
                        }
                        None => ["tag", "state", "status"]
                            .into_iter()
                            .filter_map(|s| {
                                if s.starts_with(prefix) {
                                    Some(s.to_owned())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>(),
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            _ => Ok(vec![]),
        }
    }
}
