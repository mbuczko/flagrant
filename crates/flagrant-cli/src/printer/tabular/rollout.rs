use chrono::Utc;
use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::RolloutStatus;

use super::Tabular;

impl Tabular for RolloutStatus {
    type Patch = ();
    type Context = ();

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, _ctx: &()) {
        let title = "PROGRESSIVE ROLLOUT".bold().to_string();
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
                Some(secs) => format!("{}% for {}", s.weight, format_duration(secs)),
                None => format!("{}%", s.weight),
            })
            .collect::<Vec<_>>()
            .join(" -> ");

        let is_terminal = idx + 1 >= step_count;
        let sample_ok = self.distributed_identities >= self.config.min_sample_size as i64;

        let next_str = if is_terminal {
            "rollout complete".dimmed().to_string()
        } else if idx == 0 && !sample_ok {
            format!(
                "waiting for minimum sample: {}/{}",
                self.distributed_identities, self.config.min_sample_size
            )
            .yellow()
            .to_string()
        } else {
            match current.and_then(|s| s.hold_for_secs) {
                Some(hold) => {
                    let elapsed = Utc::now()
                        .naive_utc()
                        .signed_duration_since(self.last_change_at);
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
            vec!["last change".to_string(), self.last_change_at.to_string()],
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
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(9))
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
