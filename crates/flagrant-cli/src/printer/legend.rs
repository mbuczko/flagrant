use std::borrow::Cow;

use colored::Colorize;
use fancy_table::ansi;
use terminal_size::{Width as TermWidth, terminal_size};

/// Colors `text` red if `deleted`, green if `added`, yellow if `modified` (and neither of the
/// above), or returns it unchanged otherwise - the green/red/yellow/plain scheme repeated
/// across every staged-change `Tabular` display.
pub fn stage_color<'a>(
    text: impl Into<Cow<'a, str>>,
    deleted: bool,
    added: bool,
    modified: bool,
) -> Cow<'a, str> {
    let text = text.into();
    if deleted {
        Cow::Owned(text.red().to_string())
    } else if added {
        Cow::Owned(text.green().to_string())
    } else if modified {
        Cow::Owned(text.yellow().to_string())
    } else {
        text
    }
}

/// The fixed color legend explaining what green/yellow/red mean wherever a `Tabular` impl
/// colors staged changes: green = adding, yellow = updating, red = deleting.
fn stage_legend() -> String {
    format!(
        "{}   {}   {}",
        "+ adding".green(),
        "▪ updating".yellow(),
        "✕ deleting".red(),
    )
}

/// Resolves the same width `FancyTable`'s `Width::Percentage(100)` would (full terminal
/// width, falling back to 80), so a legend line below a full-width table lines up with it.
fn table_width() -> usize {
    terminal_size()
        .map(|(TermWidth(w), _)| w as usize)
        .unwrap_or(80)
        .max(3)
}

/// Visible width of `s` ignoring ANSI escape codes (mirrors how `fancy-table` measures
/// cell content), so padding math isn't thrown off by color codes.
fn display_width(s: &str) -> usize {
    ansi::build_string(s, table_width(), 1, &ansi::Overflow::Truncate)
        .first()
        .map(|a| a.len)
        .unwrap_or(0)
}

/// Prints `left` alone, or `left` with the staged-change color legend right-aligned
/// against the table's edge (if `has_staged`), padded to match `FancyTable`'s
/// `Width::Percentage(100)`. Prints nothing at all if there's no `left` text and nothing
/// is staged, so callers don't need their own guard for that case.
pub fn print_footer(left: &str, has_staged: bool) {
    if left.is_empty() && !has_staged {
        return;
    }
    if has_staged {
        let right = stage_legend();
        let gap = table_width()
            .saturating_sub(display_width(left) + display_width(&right))
            .max(1);
        println!("{left}{}{right} \n", " ".repeat(gap - 1));
    } else {
        println!("{left}\n");
    }
}
