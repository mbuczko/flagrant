//! Adapters for the `/`-triggered context overlay: `ENVIRONMENT`/`FEATURE`/`IDENTITY`/
//! `SEGMENT`, each switching into that entity's context by name. Each just bridges the
//! REPL's `fn(&[Arg], &Session<T>)` handler signature to that domain's own `switch_to`,
//! so the actual context-switching logic (staged-changes checks, mutual exclusivity
//! between identity and segment context, etc.) stays owned by its respective handler
//! module.

use anyhow::anyhow;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};

use crate::handlers::{environments, features, identities, segments};

/// Expected args: `[name]` - omitting the name lists every environment in the project
/// (see `environments::switch_to`'s empty-string handling).
pub fn switch_environment(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args.first().map(|a| a.as_ref()).unwrap_or("");
    environments::switch_to(name, session)
}

/// Expected args: `<name>`
pub fn switch_feature(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args
        .first()
        .ok_or_else(|| anyhow!("Usage: FEATURE <name>"))?;
    features::switch_to(name, session)
}

/// Expected args: `<name>`
pub fn switch_identity(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args
        .first()
        .ok_or_else(|| anyhow!("Usage: IDENTITY <name>"))?;
    identities::switch_to(name, session)
}

/// Expected args: `<name>`
pub fn switch_segment(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args
        .first()
        .ok_or_else(|| anyhow!("Usage: SEGMENT <name>"))?;
    segments::switch_to(name, session)
}
