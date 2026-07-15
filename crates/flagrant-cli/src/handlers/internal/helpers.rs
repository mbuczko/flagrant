use std::collections::BTreeSet;

use flagrant_repl::command::Arg;

/// Extracts and concatenates all comma-separated values for a specific argument name.
///
/// Searches through command arguments for entries matching the pattern `arg:value1,value2,...`,
/// collects all unique values using a BTreeSet (which deduplicates and sorts them),
/// and returns them as a single comma-separated string.
///
/// # Arguments
/// * `arg_name` - The argument name to match (e.g., "tag", "trait", "status")
/// * `cmd_args` - Slice of command-line arguments in the format "name:value1,value2,..."
///
/// # Returns
/// A comma-separated string of all unique values found for the given argument.
///
/// # Example
/// ```ignore
/// let args = vec!["tag:foo,bar", "tag:baz,foo", "status:active"];
/// let result = concat_values_for_arg("tag", &args);
/// // result == "bar,baz,foo" (deduplicated and sorted)
/// ```
pub(crate) fn concat_values_for_arg(arg_name: &str, cmd_args: &[Arg]) -> String {
    cmd_args
        .iter()
        .fold(BTreeSet::new(), |mut acc, arg| {
            if let Some((arg, values)) = arg.split_once(":")
                && arg == arg_name
            {
                acc.extend(values.split(","));
            }
            acc
        })
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}

/// Opens `$EDITOR` (falling back to `vi`) pre-filled with `content` and returns the
/// trimmed result after the editor exits. The temp file is removed automatically.
pub(crate) fn open_in_editor(content: &str) -> anyhow::Result<String> {
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
