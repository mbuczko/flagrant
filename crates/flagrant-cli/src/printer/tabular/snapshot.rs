use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{Snapshot, SnapshotState};

use super::Tabular;

impl Tabular for Snapshot {
    type Patch = ();
    type Context = SnapshotState;

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, state: &SnapshotState) {
        let title = format!("SNAPSHOT v{}", self.version).bold().to_string();

        let status = if state.is_archived {
            format!("{} ARCHIVED", "●".dimmed())
        } else if state.is_enabled {
            format!("{} ON", "●".green())
        } else {
            format!("{} OFF", "●".red())
        };

        let variants_str = state
            .variants
            .iter()
            .map(|v| {
                let marker = if v.is_control { "★" } else { " " };
                format!("{:>3}% {marker}{}", v.weight, v.value)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let overrides_str = state
            .segment_overrides
            .iter()
            .map(|ov| {
                let weights = ov
                    .weights
                    .iter()
                    .map(|w| {
                        let value = state
                            .variants
                            .iter()
                            .find(|v| v.id == w.variant_id)
                            .map(|v| v.value.bare_first_line().to_string())
                            .unwrap_or_else(|| format!("#{}", w.variant_id));
                        format!("{value} → {}%", w.weight)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} {}", ov.segment_name.dimmed(), weights)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let identity_overrides_str = state
            .identity_overrides
            .iter()
            .map(|p| {
                let value = state
                    .variants
                    .iter()
                    .find(|v| v.id == p.variant_id)
                    .map(|v| v.value.bare_first_line().to_string())
                    .unwrap_or_else(|| format!("#{}", p.variant_id));
                format!("{} → {value}", p.identity_value.dimmed())
            })
            .collect::<Vec<_>>()
            .join(", ");

        let tags_str = state.tags.join(", ");
        let srv_str = if state.is_srv { "ON" } else { "OFF" };

        let mut rows = vec![
            vec!["name".to_string(), state.name.clone()],
            vec!["status".to_string(), status],
            vec!["variants".to_string(), variants_str],
        ];
        if !overrides_str.is_empty() {
            rows.push(vec!["segment overrides".to_string(), overrides_str]);
        }
        if !identity_overrides_str.is_empty() {
            rows.push(vec![
                "identity overrides".to_string(),
                identity_overrides_str,
            ]);
        }
        if !tags_str.is_empty() {
            rows.push(vec!["tags".to_string(), tags_str]);
        }
        if !state.description.is_empty() {
            rows.push(vec!["description".to_string(), state.description.clone()]);
        }
        rows.push(vec!["server-side".to_string(), srv_str.to_string()]);
        if let Some(comment) = &self.comment {
            rows.push(vec!["comment".to_string(), comment.clone()]);
        }
        rows.push(vec!["created".to_string(), self.created_at.to_string()]);

        let nlines: usize = rows.iter().map(|r| r[1].lines().count().max(1)).sum();

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(100),
                Align::Left,
                Overflow::Wrap,
                nlines,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
            .width(Width::Percentage(100))
            .build();

        table.render(rows);
    }
}
