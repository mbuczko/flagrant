//! `USE` - the unified context-switching command, replacing the old separate
//! `FEATURE use`, `IDENTITY use`, `SEGMENT use`, and `ENVIRONMENT use`. Dispatches on
//! the target token's leading character:
//! - `/environment` -> switch environment (doesn't combine with anything else)
//! - `@identity` -> identity context
//! - `+segment` -> segment context
//! - anything else -> feature context, optionally combined with `@identity` or
//!   `+segment` in the same token (e.g. `USE ui_theme@michal`)
//!
//! Each branch delegates to that domain's own `switch_to`, so the actual
//! context-switching logic (staged-changes checks, mutual exclusivity between identity
//! and segment context, etc.) stays owned by its respective handler module.

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};

use crate::handlers::{environments, features, identities, segments};

/// Expected args: `<feature>[@identity|+segment] | @identity | +segment | /environment`
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(target) = args.first() else {
        bail!(
            "Nothing to use. Usage: USE <feature@identity | feature+segment | @identity | +segment | /environment>"
        );
    };
    let target: &str = target;

    if let Some(env_name) = target.strip_prefix('/') {
        return environments::switch_to(env_name, session);
    }

    let (feature, identity, segment) = split_use_target(target);

    if let Some(feature) = feature {
        features::switch_to(feature, session)?;
    }
    if let Some(identity) = identity {
        identities::switch_to(identity, session)?;
    } else if let Some(segment) = segment {
        segments::switch_to(segment, session)?;
    }
    Ok(())
}

/// Splits a `USE` target into its feature/identity/segment parts: a leading `@`/`+`
/// means the whole target is a bare identity/segment switch (no feature half); otherwise
/// it's a feature name, optionally combined with a `@identity` or `+segment` shortcut in
/// the same token. At most one of identity/segment is ever returned.
fn split_use_target(target: &str) -> (Option<&str>, Option<&str>, Option<&str>) {
    if let Some(identity) = target.strip_prefix('@') {
        return (None, Some(identity), None);
    }
    if let Some(segment) = target.strip_prefix('+') {
        return (None, None, Some(segment));
    }
    if let Some((feature, identity)) = target.split_once('@') {
        return (Some(feature), Some(identity), None);
    }
    if let Some((feature, segment)) = target.split_once('+') {
        return (Some(feature), None, Some(segment));
    }
    (Some(target), None, None)
}
