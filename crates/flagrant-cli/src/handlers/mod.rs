pub mod environments;
pub mod features;
pub mod identities;
pub mod projects;
pub mod variants;

pub(crate) mod internal;

use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};

/// Commits staged changes. Delegates to the feature or identity handler
/// depending on which context is currently active.
pub fn commit(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if session.context.read().unwrap().feature.is_some() {
        features::commit(args, session)
    } else {
        identities::commit(args, session)
    }
}

/// Discards staged changes. Delegates to the feature or identity handler
/// depending on which context is currently active.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if session.context.read().unwrap().feature.is_some() {
        features::discard(args, session)
    } else {
        identities::discard(args, session)
    }
}

/// Opens `$EDITOR` (falling back to `vi`) pre-filled with `content` and returns the
/// trimmed result after the editor exits. The temp file is removed automatically.
pub(crate) fn edit_in_editor(content: &str) -> anyhow::Result<String> {
    use std::io::Write;

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(content.as_bytes())?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_owned());
    let status = std::process::Command::new(&editor)
        .arg(tmp.path())
        .status()?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status.");
    }

    let edited = std::fs::read_to_string(tmp.path())?;
    Ok(edited.trim().to_owned())
}
