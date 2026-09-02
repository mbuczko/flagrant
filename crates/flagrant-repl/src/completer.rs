use std::io;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::{Context, Result};

use crate::command::Arg;

use super::parser::{find_arg_by_position, split_command_line};

/// A list of commands with their optional operations.
/// Each entry is a tuple of (command_name, optional_operation, context_checker).
pub type CommandList<'a> = Vec<(
    String,
    &'a Option<String>,
    Option<Box<dyn Fn() -> bool + 'a>>,
)>;

/// A self-contained command list + argument completer active only when the line starts
/// with `trigger` - e.g. a `/`-triggered overlay with its own small set of commands,
/// distinct from (and never merged with) the main command list, so it can't collide with
/// unrelated op/argument completions already registered for the same command names in the
/// main list.
struct Overlay<'a> {
    trigger: char,
    commands: CommandList<'a>,
    arg_completer: Option<&'a dyn AutoCompleter>,
}

pub struct CommandLineCompleter<'a> {
    commands: CommandList<'a>,
    arg_completer: Option<&'a dyn AutoCompleter>,
    /// Trigger character and topic names for a reduced, single-token completion mode
    /// (e.g. a `?FEATURE` help overlay) - active only when the line starts with the
    /// trigger char, and offers no operation/argument completion.
    help: Option<(char, Vec<String>)>,
    /// Trigger character for a self-contained overlay command list (see [`Overlay`]).
    overlay: Option<Overlay<'a>>,
}

pub trait AutoCompleter {
    /// Returns possible completions for a command argument at a specific position.
    ///
    /// Delegates to the registered `AutoCompleter` to generate context-aware suggestions
    /// based on the command name, argument position, and the partial text already typed.
    ///
    /// # Arguments
    /// * `command` - The command being completed (e.g., "feature", "environment")
    /// * `args` - All parsed arguments from the command line
    /// * `arg_number` - Zero-based index of the argument being completed
    /// * `arg_prefix` - The partial text of the argument typed so far
    /// * `pos` - Cursor position in the input line
    ///
    /// # Returns
    /// A tuple of (cursor_position, completion_pairs) where completion_pairs contains
    /// the matching suggestions. Returns an empty list if no completions could be found.
    fn complete_by_prefix(
        &self,
        command: &str,
        args: &[Arg],
        pos: usize,
        prefix: &str,
    ) -> anyhow::Result<Vec<String>>;
}

impl<'a> CommandLineCompleter<'a> {
    /// Returns unique command completions matching the command token's prefix.
    ///
    /// Filters duplicates by assuming commands are sorted lexicographically and
    /// skipping consecutive identical command names. Matches commands that start
    /// with `prefix`, or returns all commands if `prefix` is empty.
    fn complete_command(commands: &CommandList<'_>, prefix: &str) -> anyhow::Result<Vec<Pair>> {
        let mut prev_command_str = "";
        let empty = prefix.trim().is_empty();
        let pairs = commands
            .iter()
            .filter_map(|(command_str, _, within_ctx)| {
                if command_str != prev_command_str
                    && (empty || command_str.starts_with(prefix))
                    && within_ctx.as_ref().is_none_or(|f| f())
                {
                    prev_command_str = command_str;
                    return Some(Pair {
                        display: String::default(),
                        replacement: command_str.to_owned(),
                    });
                }
                None
            })
            .collect::<Vec<_>>();

        Ok(pairs)
    }

    /// Returns operation completions for a specific command that match the given prefix.
    ///
    /// Operations are command-specific actions (e.g., "add", "list") that follow the
    /// command name (like "FEATURE"). Returns matching operations with the display form
    /// preserved and the replacement form lowercased.
    fn complete_operation(
        commands: &CommandList<'_>,
        command: &str,
        prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        let pairs = commands
            .iter()
            .filter_map(|(command_str, op, within_ctx)| {
                if command_str.eq_ignore_ascii_case(command)
                    && within_ctx.as_ref().is_none_or(|f| f())
                {
                    return match op {
                        // Op starts with prefix - candidate for completion
                        Some(op) if op.starts_with(prefix) => Some(Pair {
                            display: op.to_owned(),
                            replacement: op.to_lowercase().to_owned(),
                        }),

                        // No op or it doesn't start with op_prefix - reject
                        _ => None,
                    };
                }
                None
            })
            .collect::<Vec<_>>();

        Ok((pos, pairs))
    }

    /// Returns possible completions for a command argument at a specific position.
    ///
    /// Delegates to the registered `AutoCompleter` to generate context-aware suggestions
    /// based on the command name, argument position, and the partial text already typed.
    /// Returns an empty list if no `AutoCompleter` is registered.
    fn complete_argument(
        arg_completer: Option<&dyn AutoCompleter>,
        command: &str,
        args: &[Arg],
        arg_number: usize,
        arg_prefix: &str,
        pos: usize,
    ) -> anyhow::Result<(usize, Vec<Pair>)> {
        Ok((
            pos,
            match arg_completer {
                Some(arg_completer) => arg_completer
                    .complete_by_prefix(command, args, arg_number, arg_prefix)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| Pair {
                        replacement: s,
                        display: String::default(),
                    })
                    .collect::<Vec<_>>(),
                _ => vec![],
            },
        ))
    }

    pub fn with_arg_completer(mut self, completer: &'a dyn AutoCompleter) -> Self {
        self.arg_completer = Some(completer);
        self
    }

    /// Restricts completion to a fixed list of topic names whenever the line starts
    /// with `trigger` - no operations or arguments are offered in this mode.
    pub fn with_help_topics(mut self, trigger: char, topics: Vec<String>) -> Self {
        self.help = Some((trigger, topics));
        self
    }

    /// Registers a trigger char that activates a self-contained overlay command list -
    /// full command-name, operation, and argument completion, exactly like the main
    /// command list, but scoped to `commands`/`arg_completer` and never merged with the
    /// main ones (so command names can be reused across both without collision).
    pub fn with_overlay(
        mut self,
        trigger: char,
        commands: CommandList<'a>,
        arg_completer: Option<&'a dyn AutoCompleter>,
    ) -> Self {
        self.overlay = Some(Overlay {
            trigger,
            commands,
            arg_completer,
        });
        self
    }

    pub fn new(commands: CommandList<'a>) -> CommandLineCompleter<'a> {
        Self {
            commands,
            arg_completer: None,
            help: None,
            overlay: None,
        }
    }
}

impl Completer for CommandLineCompleter<'_> {
    type Candidate = Pair;

    /// Delegates to the registered overlay's own command list/argument completer when the
    /// line starts with its trigger char, otherwise to the main completion path.
    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(overlay) = &self.overlay
            && line.starts_with(overlay.trigger)
        {
            let trigger_len = overlay.trigger.len_utf8();
            let (start, pairs) = self.complete_scoped(
                &overlay.commands,
                overlay.arg_completer,
                &line[trigger_len..],
                pos.saturating_sub(trigger_len),
            )?;
            return Ok((start + trigger_len, pairs));
        }
        self.complete_inner(line, pos, ctx)
    }
}

impl CommandLineCompleter<'_> {
    fn complete_inner(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>)> {
        if let Some((trigger, topics)) = &self.help
            && let Some(rest) = line.strip_prefix(*trigger)
        {
            let trigger_len = trigger.len_utf8();
            let adjusted_pos = pos.saturating_sub(trigger_len);
            let args = split_command_line(rest).unwrap();
            let (arg_n, offset) = find_arg_by_position(&args, adjusted_pos);

            if arg_n == 0 {
                let start = args.first().map(|a| a.1).unwrap_or(0) + trigger_len;
                let prefix = args
                    .first()
                    .map(|a| a[..offset].to_uppercase())
                    .unwrap_or_default();
                let pairs = topics
                    .iter()
                    .filter(|t| prefix.is_empty() || t.starts_with(&prefix))
                    .map(|t| Pair {
                        display: String::default(),
                        replacement: t.clone(),
                    })
                    .collect();

                return Ok((start, pairs));
            }
            return Ok((pos, vec![]));
        }

        self.complete_scoped(&self.commands, self.arg_completer, line, pos)
    }

    /// Command-name, operation, and argument completion for a given command list - shared
    /// by the main completion path and any registered [`Overlay`], parameterized on which
    /// command list/argument completer to consult so the two can never see each other's
    /// commands.
    fn complete_scoped(
        &self,
        commands: &CommandList<'_>,
        arg_completer: Option<&dyn AutoCompleter>,
        line: &str,
        pos: usize,
    ) -> Result<(usize, Vec<Pair>)> {
        let args = split_command_line(line).unwrap();
        let (arg_n, offset) = find_arg_by_position(&args, pos);

        // Dispatch on which token the cursor is actually in (arg_n), not on how many
        // tokens the line has - otherwise editing an earlier token (e.g. moving back into
        // "FEATURE" or "use" after the line already has more words typed after it) would
        // fall through to argument completion instead of command/operation completion.
        if arg_n == 0 {
            let start = args.first().map(|a| a.1).unwrap_or(0);
            let prefix = args
                .first()
                .map(|a| a[..offset].to_uppercase())
                .unwrap_or_default();
            return Self::complete_command(commands, &prefix)
                .map(|pairs| (start, pairs))
                .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())));
        }

        let command = args.first().unwrap().as_ref();
        let argument = &args[arg_n];

        if arg_n == 1
            && let Ok(candidates) = Self::complete_operation(
                commands,
                command,
                &argument[..offset].to_lowercase(),
                argument.1,
            )
            && !candidates.1.is_empty()
        {
            return Ok(candidates);
        }

        Self::complete_argument(
            arg_completer,
            command,
            &args,
            arg_n,
            &argument[..offset],
            argument.1,
        )
        .map_err(|e| ReadlineError::Io(io::Error::other(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use rustyline::history::DefaultHistory;

    use super::*;

    #[test]
    fn completes_command_when_editing_first_token_with_more_tokens_after() {
        let op = Some("use".to_string());
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let completer = CommandLineCompleter::new(commands);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "FEAT use ui_theme" with the cursor right after "FEAT" - simulates deleting
        // "URE" from "FEATURE" while later tokens are still present on the line.
        let (start, pairs) = completer.complete("FEAT use ui_theme", 4, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(pairs.iter().any(|p| p.replacement == "FEATURE"));
    }

    #[test]
    fn completes_operation_when_editing_second_token_with_more_tokens_after() {
        let op = Some("use".to_string());
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let completer = CommandLineCompleter::new(commands);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "FEATURE us ui_theme" with the cursor right after "us" - simulates deleting
        // "e" from "use" while a 3rd token is still present on the line.
        let (_start, pairs) = completer.complete("FEATURE us ui_theme", 10, &ctx).unwrap();
        assert!(pairs.iter().any(|p| p.replacement == "use"));
    }

    #[test]
    fn completes_help_topic_with_no_space_after_trigger() {
        let completer = CommandLineCompleter::new(vec![])
            .with_help_topics('?', vec!["FEATURE".to_string(), "SEGMENT".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "?FEAT" - how it actually looks on screen, since the leading trigger char is
        // hidden from the rendered line by the overlay mechanism.
        let (start, pairs) = completer.complete("?FEAT", 5, &ctx).unwrap();
        assert_eq!(start, 1);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].replacement, "FEATURE");
    }

    #[test]
    fn completes_help_topic_with_space_after_trigger() {
        let completer = CommandLineCompleter::new(vec![])
            .with_help_topics('?', vec!["FEATURE".to_string(), "SEGMENT".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (start, pairs) = completer.complete("? FEAT", 6, &ctx).unwrap();
        assert_eq!(start, 2);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].replacement, "FEATURE");
    }

    #[test]
    fn lists_all_help_topics_for_bare_trigger() {
        let completer = CommandLineCompleter::new(vec![])
            .with_help_topics('?', vec!["FEATURE".to_string(), "SEGMENT".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_start, pairs) = completer.complete("?", 1, &ctx).unwrap();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn offers_no_completions_past_help_topic() {
        let completer = CommandLineCompleter::new(vec![])
            .with_help_topics('?', vec!["FEATURE".to_string(), "SEGMENT".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_start, pairs) = completer.complete("?FEATURE foo", 12, &ctx).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn overlay_completes_command_keyword() {
        let op: Option<String> = None;
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let completer = CommandLineCompleter::new(vec![]).with_overlay('/', commands, None);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "/FEAT" - as typed, with the leading trigger char hidden by the overlay
        // mechanism (same rendering convention as the `?` help trigger).
        let (start, pairs) = completer.complete("/FEAT", 5, &ctx).unwrap();
        assert_eq!(start, 1);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].replacement, "FEATURE");
    }

    #[test]
    fn overlay_completes_argument_via_its_own_completer() {
        struct StubCompleter;
        impl AutoCompleter for StubCompleter {
            fn complete_by_prefix(
                &self,
                command: &str,
                _args: &[Arg],
                _arg_number: usize,
                prefix: &str,
            ) -> anyhow::Result<Vec<String>> {
                assert_eq!(command, "FEATURE");
                Ok(vec![format!("{prefix}heme")])
            }
        }

        let op: Option<String> = None;
        let commands: CommandList = vec![("FEATURE".to_string(), &op, None)];
        let stub = StubCompleter;
        let completer = CommandLineCompleter::new(vec![]).with_overlay('/', commands, Some(&stub));
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // "/FEATURE ui_t" - the overlay's own argument completer is consulted, never the
        // main completer's `arg_completer` (there isn't one registered here at all).
        let (_start, pairs) = completer.complete("/FEATURE ui_t", 13, &ctx).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].replacement, "ui_theme");
    }
}
