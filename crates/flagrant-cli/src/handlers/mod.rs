pub mod environments;
pub mod features;
pub mod identities;
pub mod projects;
pub mod variants;

pub(crate) mod internal;

pub(crate) use internal::stage::{commit, discard};

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
