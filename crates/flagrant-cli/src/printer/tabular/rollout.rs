use chrono::Local;
use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::RolloutStatus;

use super::Tabular;

impl Tabular for RolloutStatus {
    type Patch = ();
    type Context = ();

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, _ctx: &()) {
        let title = "PROG. ROLLOUT".bold().to_string();
        let step_count = self.config.steps.len();
        let idx = self.current_step as usize;
        let current = self.config.steps.get(idx);

        let step_str = match current {
            Some(step) => format!("{} of {} ({}%)", idx + 1, step_count, step.weight),
            None => "unknown".to_string(),
        };

        let schedule_str = self
            .config
            .steps
            .iter()
            .map(|s| match s.hold_for_secs {
                Some(secs) => format!("{}% for {}", s.weight, format_duration(secs))
                    .white()
                    .to_string(),
                None => format!("{}%", s.weight).white().to_string(),
            })
            .collect::<Vec<_>>()
            .join(" => ");

        let is_terminal = idx + 1 >= step_count;
        let sample_reached = self.distributed_identities >= self.config.min_sample_size as i64;

        // `last_change_at` is a naive UTC timestamp (the server records it via
        // `Utc::now().naive_utc()`) - anchor it to `Utc` explicitly, then convert to the
        // system's local timezone, so both the elapsed-time math and the displayed
        // timestamp below reflect the user's actual clock/timezone settings rather than
        // silently showing UTC as if it were local.
        let last_change_local = self.last_change_at.and_utc().with_timezone(&Local);

        let next_str = if is_terminal {
            "rollout complete".dimmed().to_string()
        } else if idx == 0 && !sample_reached {
            format!(
                "waiting for minimum sample: {}/{}",
                self.distributed_identities, self.config.min_sample_size
            )
            .yellow()
            .to_string()
        } else {
            match current.and_then(|s| s.hold_for_secs) {
                Some(hold) => {
                    let elapsed = Local::now().signed_duration_since(last_change_local);
                    let remaining = hold as i64 - elapsed.num_seconds();

                    if remaining <= 0 {
                        "due - will advance on next read".green().to_string()
                    } else {
                        format!("in ~{}", format_duration(remaining as u32))
                    }
                }
                None => "unknown".to_string(),
            }
        };

        let sample_str = format!(
            "{} (min {} required to start)",
            self.distributed_identities, self.config.min_sample_size
        );

        let rows = vec![
            vec!["step".to_string(), step_str],
            vec!["schedule".to_string(), schedule_str],
            vec![
                "last change".to_string(),
                last_change_local.format("%Y-%m-%d %H:%M:%S").to_string(),
            ],
            vec!["next step".to_string(), next_str],
            vec!["sample".to_string(), sample_str],
        ];

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(100),
                Align::Left,
                Overflow::Wrap,
                5,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(4))
            .width(Width::Percentage(100))
            .build();

        table.render(rows);
    }
}

/// Formats a duration in seconds as a compact human string (e.g. `1d 2h 3m`), used both
/// for the schedule summary and the time-to-next-step estimate.
fn format_duration(mut secs: u32) -> String {
    let days = secs / 86400;
    secs %= 86400;
    let hours = secs / 3600;
    secs %= 3600;
    let minutes = secs / 60;
    secs %= 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs}s"));
    }
    parts.join(" ")
}
