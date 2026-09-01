use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, completer::AutoCompleter, session::Session};
use flagrant_types::{Comparator, Environment, Feature, IdentityWithTraits, Segment, Tag, Trait};
use strum::IntoEnumIterator;

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
                complete_environments(&ctx, prefix)
            }
            "IDENTITY" => {
                let op: &str = &args[1];
                let ctx = self.session.context.read().unwrap();

                Ok(match op {
                    "add" if arg_n >= 3 && !prefix.contains('=') => complete_traits(&ctx, prefix)?
                        .into_iter()
                        .map(|name| format!("{name}="))
                        .collect::<Vec<_>>(),
                    // Auto-complete trait names for `IDENTITY trait`. A `-` prefix means
                    // removal, so it's carried through without appending `=` (no value needed).
                    "trait" if arg_n >= 2 && !prefix.contains('=') => {
                        let (modifier, val) = match prefix.strip_prefix('-') {
                            Some(rest) => (Some('-'), rest),
                            None => (None, prefix),
                        };

                        complete_traits(&ctx, val)?
                            .into_iter()
                            .map(|name| {
                                let mut out = String::new();
                                if let Some(m) = modifier {
                                    out.push(m);
                                }
                                out.push_str(&name);
                                if modifier.is_none() {
                                    out.push('=');
                                }
                                out
                            })
                            .collect::<Vec<_>>()
                    }
                    "delete" | "show" if arg_n == 2 => complete_identities(&ctx, prefix)?,
                    // Auto-complete trait names for filtering, e.g. `trait:vip` or `trait:-vip`
                    "list" => match prefix.split_once(':') {
                        Some(("trait", val)) => {
                            let (lhs, modifier, val) = strip_tag(val);

                            complete_traits(&ctx, val)?
                                .into_iter()
                                .map(|name| {
                                    let mut out = String::with_capacity(name.len() + 2);
                                    if !lhs.is_empty() {
                                        out.push(',');
                                    }
                                    if let Some(m) = modifier {
                                        out.push(m);
                                    }
                                    out.push_str(&name);
                                    format!("trait:{lhs}{out}")
                                })
                                .collect::<Vec<_>>()
                        }
                        None => filter_by_prefix(&["trait"], prefix),
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            "FEATURE" if arg_n >= 2 => {
                let ctx = self.session.context.read().unwrap();
                let res = ctx.env_resource();
                let op: &str = &args[1];

                Ok(match op {
                    // Auto-complete feature name
                    "delete" | "show" if arg_n == 2 => complete_features(&ctx, prefix)?,

                    "status" if arg_n == 2 => filter_by_prefix(&["on", "off", "archived"], prefix),
                    "server-side" if arg_n == 2 => filter_by_prefix(&["on", "off"], prefix),
                    "progressive" if arg_n == 2 => {
                        filter_by_prefix(&["rules", "sample", "delete", "status"], prefix)
                    }
                    // Auto-complete tag names for `FEATURE tag`. A `-` prefix means removal.
                    "tag" if arg_n >= 2 => {
                        let (modifier, val) = match prefix.strip_prefix('-') {
                            Some(rest) => (Some('-'), rest),
                            None => (None, prefix),
                        };

                        ctx.client
                            .get::<Vec<Tag>>(res.subpath(format!("/tags?prefix={val}")))?
                            .into_iter()
                            .map(|t| match modifier {
                                Some(m) => format!("{m}{}", t.name),
                                None => t.name,
                            })
                            .collect::<Vec<_>>()
                    }

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
                        Some(("status", val)) => filter_by_prefix(&["on", "off", "archived"], val)
                            .into_iter()
                            .map(|v| format!("status:{v}"))
                            .collect::<Vec<_>>(),
                        None => filter_by_prefix(&["tag", "status"], prefix),
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            // Auto-complete the `USE` target: `@identity`, `+segment`, a bare feature
            // name, or `feature@identity` / `feature+segment` combining them.
            "USE" if arg_n == 1 => {
                let ctx = self.session.context.read().unwrap();

                Ok(if let Some(env_prefix) = prefix.strip_prefix('/') {
                    complete_environments(&ctx, env_prefix)?
                        .into_iter()
                        .map(|v| format!("/{v}"))
                        .collect::<Vec<_>>()
                } else if let Some((feature_part, identity_prefix)) = prefix.split_once('@') {
                    complete_identities(&ctx, identity_prefix)?
                        .into_iter()
                        .map(|v| format!("{feature_part}@{v}"))
                        .collect::<Vec<_>>()
                } else if let Some((feature_part, segment_prefix)) = prefix.split_once('+') {
                    complete_segments(&ctx, segment_prefix)?
                        .into_iter()
                        .map(|v| format!("{feature_part}+{v}"))
                        .collect::<Vec<_>>()
                } else {
                    complete_features(&ctx, prefix)?
                })
            }
            // Auto-complete feature name, or `feature@identity`, for the `GET` test
            // command - same `@` split as `USE` above.
            "GET" if arg_n == 1 => {
                let ctx = self.session.context.read().unwrap();

                Ok(match prefix.split_once('@') {
                    Some((feature_part, identity_prefix)) => {
                        complete_identities(&ctx, identity_prefix)?
                            .into_iter()
                            .map(|v| format!("{feature_part}@{v}"))
                            .collect::<Vec<_>>()
                    }
                    None => complete_features(&ctx, prefix)?,
                })
            }
            // Auto-complete `@identity` for the `GETALL` test command - only once `@` has
            // been typed, since (unlike GET) GETALL has no bare-token form.
            "GETALL" if arg_n == 1 => {
                let ctx = self.session.context.read().unwrap();

                Ok(match prefix.strip_prefix('@') {
                    Some(identity_prefix) => complete_identities(&ctx, identity_prefix)?
                        .into_iter()
                        .map(|v| format!("@{v}"))
                        .collect::<Vec<_>>(),
                    None => vec![],
                })
            }
            "RULE" if arg_n >= 2 => {
                let op: &str = &args[1];
                let ctx = self.session.context.read().unwrap();

                Ok(match op {
                    // Since group labels are auto-generated, let's simplify the autocompletion
                    // and reduce it to "group-" only.
                    "add" | "show" | "delete" | "value" | "comparator" if arg_n == 2 => {
                        filter_by_prefix(&["group-"], prefix)
                    }
                    "add" if arg_n == 3 => {
                        if let Some(trait_prefix) = prefix.strip_prefix("trait:") {
                            complete_traits(&ctx, trait_prefix)?
                                .into_iter()
                                .map(|name| format!("trait:{name}"))
                                .collect()
                        } else {
                            filter_by_prefix(&["identity", "trait:", "environment"], prefix)
                        }
                    }
                    "add" if arg_n == 4 => Comparator::iter()
                        .map(|c| c.to_string())
                        .filter(|s| s.starts_with(prefix))
                        .collect(),
                    "add" if arg_n == 5 => match args.get(3).map(|a| a.as_ref()) {
                        Some("identity") => complete_identities(&ctx, prefix)?,
                        Some("environment") => complete_environments(&ctx, prefix)?,
                        _ => vec![],
                    },
                    _ => vec![],
                })
            }
            "GROUP" if arg_n >= 2 => {
                let op: &str = &args[1];
                Ok(match op {
                    "add" if arg_n == 2 => filter_by_prefix(&["--and", "--and-not"], prefix),
                    "delete" | "describe" | "rejoin" | "show" if arg_n == 2 => {
                        filter_by_prefix(&["group-"], prefix)
                    }
                    _ => vec![],
                })
            }
            "SEGMENT" if arg_n >= 2 => {
                let ctx = self.session.context.read().unwrap();
                let res = ctx.project_resource();
                let op: &str = &args[1];

                Ok(match op {
                    "delete" | "show" if arg_n == 2 => complete_segments(&ctx, prefix)?,
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

/// Completes identity values (bare, unformatted) matching `prefix`, scoped to the
/// current environment. Shared by every command completing an identity token.
fn complete_identities(ctx: &Connection, prefix: &str) -> anyhow::Result<Vec<String>> {
    Ok(ctx
        .client
        .get::<Vec<IdentityWithTraits>>(
            ctx.env_resource()
                .subpath(format!("/identities?prefix={prefix}")),
        )?
        .into_iter()
        .map(|i| i.value)
        .collect())
}

/// Completes feature names (bare, unformatted) matching `prefix`, scoped to the
/// current environment. Shared by every command completing a feature token.
fn complete_features(ctx: &Connection, prefix: &str) -> anyhow::Result<Vec<String>> {
    Ok(ctx
        .client
        .get::<Vec<Feature>>(
            ctx.env_resource()
                .subpath(format!("/features?prefix={prefix}")),
        )?
        .into_iter()
        .map(|c| c.name)
        .collect())
}

/// Completes environment names (bare, unformatted) matching `prefix`, scoped to the
/// current project. Shared by every command completing an environment token.
fn complete_environments(ctx: &Connection, prefix: &str) -> anyhow::Result<Vec<String>> {
    Ok(ctx
        .client
        .get::<Vec<Environment>>(
            ctx.project
                .as_base_resource()
                .subpath(format!("/envs?prefix={prefix}")),
        )?
        .into_iter()
        .map(|e| e.name)
        .collect())
}

/// Completes segment names (bare, unformatted) matching `prefix`, scoped to the current
/// project. Shared by every command completing a segment token.
fn complete_segments(ctx: &Connection, prefix: &str) -> anyhow::Result<Vec<String>> {
    Ok(ctx
        .client
        .get::<Vec<Segment>>(
            ctx.project_resource()
                .subpath(format!("/segments?prefix={prefix}")),
        )?
        .into_iter()
        .map(|s| s.name)
        .collect())
}

/// Completes trait names (bare, unformatted) matching `prefix`, scoped to the current
/// project. Shared by every command completing a trait token.
fn complete_traits(ctx: &Connection, prefix: &str) -> anyhow::Result<Vec<String>> {
    Ok(ctx
        .client
        .get::<Vec<Trait>>(
            ctx.project
                .as_base_resource()
                .subpath(format!("/traits?prefix={prefix}")),
        )?
        .into_iter()
        .map(|t| t.name)
        .collect())
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
