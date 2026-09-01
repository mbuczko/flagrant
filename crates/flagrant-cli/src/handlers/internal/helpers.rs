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

/// Prompts inline for a single line of free-text input, pre-filled with `current` so it
/// can be edited in place.
///
/// Returns `Ok(None)` if the user cancels (Ctrl-C/Ctrl-D).
pub(crate) fn prompt_line(prompt: &str, current: &str) -> anyhow::Result<Option<String>> {
    let mut rl = rustyline::DefaultEditor::new()?;

    match rl.readline_with_initial(&format!("{prompt}: "), (current, "")) {
        Ok(line) => Ok(Some(line.trim().to_string())),
        Err(rustyline::error::ReadlineError::Interrupted)
        | Err(rustyline::error::ReadlineError::Eof) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
