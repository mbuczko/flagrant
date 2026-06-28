use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, completer::AutoCompleter, session::Session};
use flagrant_types::{Environment, Feature, IdentityWithTraits, Segment, Tag, Trait};

pub struct ArgCompleter<'a> {
    pub session: &'a Session<Connection>,
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

                // Auto-complete environment name
                Ok(ctx
                    .client
                    .get::<Vec<Environment>>(res.subpath(format!("/envs?prefix={prefix}")))?
                    .into_iter()
                    .map(|c| c.name)
                    .collect::<Vec<_>>())
            }
            "IDENTITY" => {
                let op: &str = &args[1];
                let ctx = self.session.context.read().unwrap();
                let project_res = ctx.project.as_base_resource();
                let env_res = ctx.env_resource();

                Ok(match op {
                    "add" if arg_n >= 3 && !prefix.contains(':') => ctx
                        .client
                        .get::<Vec<Trait>>(project_res.subpath(format!("/traits?prefix={prefix}")))?
                        .into_iter()
                        .map(|t| format!("{}:", t.name))
                        .collect::<Vec<_>>(),
                    "delete" | "describe" | "use" if arg_n == 2 => ctx
                        .client
                        .get::<Vec<IdentityWithTraits>>(
                            env_res.subpath(format!("/identities?prefix={prefix}")),
                        )?
                        .into_iter()
                        .map(|c| c.value)
                        .collect::<Vec<_>>(),
                    _ => vec![],
                })
            }
            "SET" if arg_n >= 2 => {
                let op: &str = &args[1];

                Ok(match op {
                    "status" => filter_by_prefix(&["on", "off", "archived"], prefix),
                    "trait" if arg_n == 2 && !prefix.contains(':') => {
                        let ctx = self.session.context.read().unwrap();
                        let res = ctx.project.as_base_resource();

                        ctx.client
                            .get::<Vec<Trait>>(res.subpath(format!("/traits?prefix={prefix}")))?
                            .into_iter()
                            .map(|t| format!("{}:", t.name))
                            .collect::<Vec<_>>()
                    }
                    _ => vec![],
                })
            }
            "UNSET" if arg_n >= 2 => {
                let op: &str = &args[1];

                Ok(match op {
                    "trait" if arg_n == 2 => {
                        let ctx = self.session.context.read().unwrap();
                        let res = ctx.project.as_base_resource();

                        ctx.client
                            .get::<Vec<Trait>>(res.subpath(format!("/traits?prefix={prefix}")))?
                            .into_iter()
                            .map(|t| t.name)
                            .collect::<Vec<_>>()
                    }
                    _ => vec![],
                })
            }
            "FEATURE" if arg_n >= 2 => {
                let ctx = self.session.context.read().unwrap();
                let res = ctx.env_resource();
                let op: &str = &args[1];

                Ok(match op {
                    // Auto-complete feature name, or `feature@identity` when prefix contains `@`
                    "use" if arg_n == 2 => {
                        if let Some((feature_part, identity_prefix)) = prefix.split_once('@') {
                            ctx.client
                                .get::<Vec<IdentityWithTraits>>(
                                    ctx.env_resource()
                                        .subpath(format!("/identities?prefix={identity_prefix}")),
                                )?
                                .into_iter()
                                .map(|i| format!("{feature_part}@{}", i.value))
                                .collect::<Vec<_>>()
                        } else {
                            ctx.client
                                .get::<Vec<Feature>>(
                                    res.subpath(format!("/features?prefix={prefix}")),
                                )?
                                .into_iter()
                                .map(|c| c.name)
                                .collect::<Vec<_>>()
                        }
                    }
                    // Auto-complete feature name
                    "delete" | "describe" if arg_n == 2 => ctx
                        .client
                        .get::<Vec<Feature>>(res.subpath(format!("/features?prefix={prefix}")))?
                        .into_iter()
                        .map(|c| c.name)
                        .collect::<Vec<_>>(),

                    // Auto-complete feature attribute names like tags or status,
                    // along with the attribute value (if completable)
                    "list" => match prefix.split_once(':') {
                        Some(("tag", val)) => {
                            let ctx = self.session.context.read().unwrap();
                            let res = ctx.env_resource();
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
                        None => filter_by_prefix(&["tag", "enabled", "archived"], prefix),
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            "RULE" if arg_n >= 2 => {
                let op: &str = &args[1];
                let ctx = self.session.context.read().unwrap();

                Ok(match op {
                    "add" if arg_n == 3 => {
                        if let Some(trait_prefix) = prefix.strip_prefix("trait:") {
                            let res = ctx.project.as_base_resource();
                            ctx.client
                                .get::<Vec<Trait>>(
                                    res.subpath(format!("/traits?prefix={trait_prefix}")),
                                )?
                                .into_iter()
                                .map(|t| format!("trait:{}", t.name))
                                .collect()
                        } else {
                            filter_by_prefix(&["identity", "trait:", "environment"], prefix)
                        }
                    }
                    "add" if arg_n == 4 => filter_by_prefix(
                        &[
                            "exactly-matches",
                            "does-not-match",
                            "contains",
                            "does-not-contain",
                            "greater-than",
                            "greater-equal-than",
                            "lower-than",
                            "lower-equal-than",
                            "in",
                            "not-in",
                        ],
                        prefix,
                    ),
                    "add" if arg_n == 5 => match args.get(3).map(|a| a.as_ref()) {
                        Some("identity") => {
                            let env_res = ctx.env_resource();
                            ctx.client
                                .get::<Vec<IdentityWithTraits>>(
                                    env_res.subpath(format!("/identities?prefix={prefix}")),
                                )?
                                .into_iter()
                                .map(|i| i.value)
                                .collect()
                        }
                        Some("environment") => {
                            let project_res = ctx.project.as_base_resource();
                            ctx.client
                                .get::<Vec<Environment>>(
                                    project_res.subpath(format!("/envs?prefix={prefix}")),
                                )?
                                .into_iter()
                                .map(|e| e.name)
                                .collect()
                        }
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            "SEGMENT" if arg_n >= 2 => {
                let ctx = self.session.context.read().unwrap();
                let res = ctx.project_resource();
                let op: &str = &args[1];

                Ok(match op {
                    "use" | "describe" | "delete" if arg_n == 2 => ctx
                        .client
                        .get::<Vec<Segment>>(res.subpath(format!("/segments?prefix={prefix}")))?
                        .into_iter()
                        .map(|s| s.name)
                        .collect::<Vec<_>>(),
                    "list" if arg_n == 2 => ctx
                        .client
                        .get::<Vec<Segment>>(res.subpath(format!("/segments?pattern={prefix}")))?
                        .into_iter()
                        .map(|s| s.name)
                        .collect::<Vec<_>>(),
                    _ => vec![],
                })
            }
            _ => Ok(vec![]),
        }
    }
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

fn filter_by_prefix<'a>(candidates: &[&'a str], prefix: &'a str) -> Vec<String> {
    candidates
        .iter()
        .filter_map(|s| {
            if s.starts_with(prefix) {
                Some(s.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}
