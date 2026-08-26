use std::io::{IsTerminal, Write, stdout};

use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    terminal::{self, ClearType},
};

const VISIBLE_ROWS: usize = 10;

/// Right-pads each `prefix` to the width of the widest one, then joins it with `" : "` and
/// its `suffix` - so a list of options sharing a prefix shape (e.g. `"variant #1 (30%)"`,
/// `"variant #2 (100%)"`) lines up on the colon despite the prefixes differing in width.
pub fn align_rows(rows: &[(String, String)]) -> Vec<String> {
    let width = rows
        .iter()
        .map(|(prefix, _)| prefix.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(prefix, suffix)| format!("{prefix:width$} : {suffix}"))
        .collect()
}

/// Restores the terminal (raw mode + cursor visibility) when dropped, so it's put back no
/// matter which path `select` returns through - a confirmed pick, a cancel, or a bailed-out
/// error - keeping it safe to run between two `rustyline` `readline()` calls.
struct RawModeGuard;

impl RawModeGuard {
    fn new() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

/// Interactively prompts the user to pick one of `options` (label, value) using the
/// keyboard - Up/Down or j/k to move (wrapping at the ends), Enter to confirm, Esc/q/Ctrl+C
/// to cancel. Shows at most 10 options at a time, scrolling with `↑/↓ N more` indicators
/// when there are more. `default` pre-selects an index. Returns `Ok(None)` on cancel.
pub fn select<T: Clone>(
    prompt: &str,
    options: &[(String, T)],
    default: Option<usize>,
) -> anyhow::Result<Option<T>> {
    if options.is_empty() {
        return Ok(None);
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal.");
    }

    let mut selected = default.unwrap_or(0).min(options.len() - 1);
    let _guard = RawModeGuard::new()?;
    let mut previous_lines = 0u16;

    loop {
        let lines = render(prompt, options, selected, previous_lines)?;
        previous_lines = lines;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = if selected == 0 {
                        options.len() - 1
                    } else {
                        selected - 1
                    };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1) % options.len();
                }
                KeyCode::Enter => {
                    clear(previous_lines)?;
                    return Ok(Some(options[selected].1.clone()));
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    clear(previous_lines)?;
                    return Ok(None);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    clear(previous_lines)?;
                    return Ok(None);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Redraws the whole frame in place: moves the cursor up over the previous frame (if any)
/// and clears everything below before repainting, so frames don't stack up in scrollback.
/// Returns how many lines this frame took, for the next redraw to clear.
fn render<T>(
    prompt: &str,
    options: &[(String, T)],
    selected: usize,
    previous_lines: u16,
) -> anyhow::Result<u16> {
    let mut out = stdout();

    if previous_lines > 0 {
        queue!(out, cursor::MoveUp(previous_lines))?;
    }
    queue!(out, terminal::Clear(ClearType::FromCursorDown))?;

    let mut lines = 0u16;

    writeln!(out, "{}\r", prompt.bold())?;
    lines += 1;

    let window_start = window_start(options.len(), selected, VISIBLE_ROWS);
    let window_end = (window_start + VISIBLE_ROWS).min(options.len());

    if window_start > 0 {
        writeln!(out, "{}\r", format!("  ↑ {window_start} more").dimmed())?;
        lines += 1;
    }

    for (i, (label, _)) in options
        .iter()
        .enumerate()
        .take(window_end)
        .skip(window_start)
    {
        if i == selected {
            writeln!(out, "{}\r", format!("▸ {label}").green().bold())?;
        } else {
            writeln!(out, "  {label}\r")?;
        }
        lines += 1;
    }

    if window_end < options.len() {
        let remaining = options.len() - window_end;
        writeln!(out, "{}\r", format!("  ↓ {remaining} more").dimmed())?;
        lines += 1;
    }

    writeln!(out, "{}\r", "↑/↓ move   Enter select   Esc cancel".dimmed())?;
    lines += 1;

    out.flush()?;
    Ok(lines)
}

/// One adjustable row in [`adjust_weights`]: `suffix` is the fixed display text shown
/// after the weight percentage (e.g. `"my-value (staged)"`); `weight` is mutated in place
/// as the user adjusts it.
pub struct WeightRow {
    pub suffix: String,
    pub weight: u8,
}

/// Interactively edits `rows`' weights in place, for the "adjust a segment override" UI.
/// Up/Down (or j/k) moves the highlighted row, Left/Right (or h/l) adjusts its weight by 5,
/// clamped to `[0, 100]` and so the total across all rows never exceeds 100. Enter confirms
/// (`Ok(true)`); Esc/q/Ctrl+C cancels (`Ok(false)`; `rows` may already reflect in-progress
/// adjustments at that point and should be discarded by the caller). A trailing,
/// non-interactive row labeled `remainder_suffix` always shows the automatic
/// `100 - sum(rows)` remainder, mirroring the control/default variant's auto-balanced
/// weight.
pub fn adjust_weights(
    prompt: &str,
    rows: &mut [WeightRow],
    remainder_suffix: &str,
) -> anyhow::Result<bool> {
    if rows.is_empty() {
        return Ok(false);
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal.");
    }

    let mut selected = 0usize;
    let _guard = RawModeGuard::new()?;
    let mut previous_lines = 0u16;

    loop {
        let lines = render_weights(prompt, rows, remainder_suffix, selected, previous_lines)?;
        previous_lines = lines;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = if selected == 0 {
                        rows.len() - 1
                    } else {
                        selected - 1
                    };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1) % rows.len();
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    rows[selected].weight = rows[selected].weight.saturating_sub(5);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let others: u32 = rows
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != selected)
                        .map(|(_, r)| r.weight as u32)
                        .sum();
                    let max_allowed = 100u32.saturating_sub(others) as u8;
                    rows[selected].weight =
                        rows[selected].weight.saturating_add(5).min(max_allowed);
                }
                KeyCode::Enter => {
                    clear(previous_lines)?;
                    return Ok(true);
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    clear(previous_lines)?;
                    return Ok(false);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    clear(previous_lines)?;
                    return Ok(false);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Redraws the weight-adjustment frame in place, same MoveUp+Clear+repaint scheme as
/// [`render`]. `rows` are numbered and colon-aligned together with the trailing,
/// always-shown remainder row (`100 - sum(rows)`, labeled `remainder_suffix`), which isn't
/// part of the scrollable window since it's never selectable.
fn render_weights(
    prompt: &str,
    rows: &[WeightRow],
    remainder_suffix: &str,
    selected: usize,
    previous_lines: u16,
) -> anyhow::Result<u16> {
    let mut out = stdout();

    if previous_lines > 0 {
        queue!(out, cursor::MoveUp(previous_lines))?;
    }
    queue!(out, terminal::Clear(ClearType::FromCursorDown))?;

    let mut lines = 0u16;

    writeln!(out, "{}\r", prompt.bold())?;
    lines += 1;

    // Reserve one visible row for the always-shown remainder row.
    let adjustable_visible = VISIBLE_ROWS.saturating_sub(1).max(1);
    let window_start = window_start(rows.len(), selected, adjustable_visible);
    let window_end = (window_start + adjustable_visible).min(rows.len());

    let sum: u32 = rows.iter().map(|r| r.weight as u32).sum();
    let remainder = 100u32.saturating_sub(sum) as u8;

    let mut pairs: Vec<(String, String)> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                format!("variant #{} ({}%)", i + 1, r.weight),
                r.suffix.clone(),
            )
        })
        .collect();
    pairs.push((
        format!("variant #{} ({remainder}%)", rows.len() + 1),
        remainder_suffix.to_string(),
    ));
    let aligned = align_rows(&pairs);

    if window_start > 0 {
        writeln!(out, "{}\r", format!("  ↑ {window_start} more").dimmed())?;
        lines += 1;
    }

    for (i, label) in aligned
        .iter()
        .enumerate()
        .take(window_end)
        .skip(window_start)
    {
        if i == selected {
            writeln!(out, "{}\r", format!("▸ {label}").green().bold())?;
        } else {
            writeln!(out, "  {label}\r")?;
        }
        lines += 1;
    }

    if window_end < rows.len() {
        let remaining = rows.len() - window_end;
        writeln!(out, "{}\r", format!("  ↓ {remaining} more").dimmed())?;
        lines += 1;
    }

    writeln!(out, "{}\r", format!("  {}", aligned[rows.len()]).dimmed())?;
    lines += 1;

    writeln!(
        out,
        "{}\r",
        "↑/↓ move   ←/→ adjust ±5%   Enter confirm   Esc cancel".dimmed()
    )?;
    lines += 1;

    out.flush()?;
    Ok(lines)
}

/// Keeps `selected` within a `visible`-sized window, scrolling only as far as needed.
fn window_start(len: usize, selected: usize, visible: usize) -> usize {
    if len <= visible {
        return 0;
    }
    selected.saturating_sub(visible - 1).min(len - visible)
}

fn clear(previous_lines: u16) -> anyhow::Result<()> {
    let mut out = stdout();
    if previous_lines > 0 {
        queue!(out, cursor::MoveUp(previous_lines))?;
    }
    queue!(out, terminal::Clear(ClearType::FromCursorDown))?;
    out.flush()?;
    Ok(())
}
