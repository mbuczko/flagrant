use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    GroupConnector, OverriddenVariant, Segment, SegmentFeatureOverride,
    payload::{SegmentPatch, SegmentPatchOp},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

/// Context passed to `Segment::display` to show the features this segment overrides.
#[derive(Default)]
pub struct SegmentContext {
    pub overrides: Vec<SegmentFeatureOverride>,
}

impl Tabular for Segment {
    type Patch = SegmentPatch;
    type Context = SegmentContext;

    fn list(selfs: &[Self]) {
        if selfs.is_empty() {
            println!("No segments found.");
            return;
        }
        let rows: Vec<_> = selfs
            .iter()
            .map(|seg| {
                [
                    seg.name.clone(),
                    seg.description.clone().unwrap_or_default(),
                    format!("{} group(s)", seg.groups.len()),
                ]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(50), Align::Left)
            .add_column_named_with_align("GROUPS".into(), Layout::Fixed(12), Align::Left)
            .width(Width::Percentage(100))
            .build()
            .render(rows);
    }

    fn display(&self, patch: Option<&SegmentPatch>, ctx: &SegmentContext) {
        let eff = effective::effective_segment(self, patch);
        let title = "SEGMENT".bold().to_string();

        let is_deleted =
            patch.is_some_and(|p| p.ops.iter().any(|op| matches!(op, SegmentPatchOp::Delete)));

        let (name_str, name_stage) = if is_deleted {
            (eff.name.red().to_string(), "✕ deleting".red().to_string())
        } else if eff.name_modified {
            (
                eff.name.yellow().to_string(),
                "‣ updating".yellow().to_string(),
            )
        } else {
            (eff.name, String::new())
        };
        let (desc_str, desc_stage) = if eff.description_modified {
            (
                eff.description.unwrap_or_default().yellow().to_string(),
                "‣ updating".yellow().to_string(),
            )
        } else {
            (eff.description.unwrap_or_default(), String::new())
        };

        // Upper-bound capacity: each group pushes at most 3 lines for its connector, 1 for
        // the label, 1 for the closing corner, plus one per rule (the +1 also safely covers
        // the single "(no rules)" placeholder line when a group has none).
        let group_capacity: usize = eff.groups.iter().map(|g| g.rules.len() + 6).sum();
        let mut group_lines: Vec<String> = Vec::with_capacity(group_capacity);
        let mut group_stage: Vec<String> = Vec::with_capacity(group_capacity);

        for group in &eff.groups {
            if let Some(connector) = &group.connector {
                let sym = format_connector(connector);
                let sym_colored = if is_deleted || group.is_deleted {
                    sym.red()
                } else if group.is_staged_add {
                    sym.green()
                } else {
                    sym.bright_cyan()
                };
                // Three separate pushes rather than one entry with embedded "\n"s: the
                // stage column height is set to group_stage.len(), so every visual line
                // must correspond to exactly one vector element. Otherwise the column is
                // sized too small and trailing annotations (e.g. "deleting") get cut off.
                group_lines.push(String::new());
                group_stage.push(String::new());
                group_lines.push(format!(" {sym_colored}"));
                group_stage.push(String::new());
                group_lines.push(String::new());
                group_stage.push(String::new());
            }

            let (body_lines, body_stage) = super::group::group_body_lines(group, is_deleted);
            group_lines.extend(body_lines);
            group_stage.extend(body_stage);
        }

        // overrides_lines and overrides_stages must stay in lockstep (one stage entry per
        // content line) since they're joined by "\n" and rendered as aligned rows in
        // adjacent table columns.
        let mut overrides_lines: Vec<String> = Vec::new();
        let mut overrides_stages: Vec<String> = Vec::new();

        for o in &ctx.overrides {
            let pending_op = patch.into_iter().flat_map(|p| &p.ops).find(|op| {
                matches!(op,
                    SegmentPatchOp::SetFeatureOverride { feature_id, .. }
                    | SegmentPatchOp::UnsetFeatureOverride { feature_id, .. }
                    if *feature_id == o.feature_id
                )
            });

            let parts = overridden_variant_parts(&o.weights);
            let plain_line = format!(
                "{} › {} {}",
                "feature".bright_blue(),
                o.feature_name.dimmed(),
                parts.join(", ")
            );

            match pending_op {
                Some(SegmentPatchOp::UnsetFeatureOverride { .. }) => {
                    overrides_lines.push(plain_line.red().to_string());
                    overrides_stages.push("‣ removing".red().to_string());
                }
                Some(SegmentPatchOp::SetFeatureOverride { .. }) => {
                    overrides_lines.push(plain_line.yellow().to_string());
                    overrides_stages.push("‣ updating".yellow().to_string());
                }
                _ => {
                    overrides_lines.push(plain_line);
                    overrides_stages.push(String::new());
                }
            }
        }

        let has_staged_overrides = overrides_stages.iter().any(|s| !s.is_empty());
        let has_staged = !name_stage.is_empty()
            || !desc_stage.is_empty()
            || group_stage.iter().any(|s| !s.is_empty())
            || has_staged_overrides;

        let table = if has_staged {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    20,
                )
                .add_column(
                    None,
                    Layout::Fixed(14),
                    Align::Left,
                    Overflow::Truncate,
                    group_stage.len().max(1),
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
                .width(Width::Percentage(100))
                .build()
        } else {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    20,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
                .width(Width::Percentage(100))
                .build()
        };

        let mut rows = vec![
            vec!["name".to_string(), name_str, name_stage],
            vec![
                "rules".to_string(),
                group_lines.join("\n"),
                group_stage.join("\n"),
            ],
        ];
        if let overrides_str = overrides_lines.join("\n")
            && !overrides_str.is_empty()
        {
            rows.push(vec![
                "overrides".to_string(),
                overrides_str,
                overrides_stages.join("\n"),
            ]);
        }
        rows.push(vec!["description".to_string(), desc_str, desc_stage]);

        // The table itself only has a stage column when has_staged, so drop it from each
        // row to match - it would be all empty strings anyway in that case.
        if !has_staged {
            for row in &mut rows {
                row.truncate(2);
            }
        }
        table.render(rows);

        if !overrides_lines.is_empty() {
            println!("  {} control variant\n", "★".dimmed());
        }
        if !(eff.groups.iter().any(|g| !g.is_deleted || g.is_staged_add)) {
            println!(
                "{}",
                "(no group added yet - use `GROUP add ...` to create one)".dimmed()
            );
        }
    }
}

fn overridden_variant_parts(weights: &[OverriddenVariant]) -> Vec<String> {
    weights
        .iter()
        .map(|w| {
            let (_, bare) = w.value.decompose();
            let first_line = bare.lines().next().unwrap_or(bare);
            let marker = if w.is_control { "★" } else { "" };
            format!("{marker}{first_line} → {}", format!("{}%", w.weight).bold())
        })
        .collect()
}

pub(super) fn format_connector(connector: &GroupConnector) -> &'static str {
    match connector {
        GroupConnector::And => "⊕ AND",
        GroupConnector::AndNot => "⊖ AND NOT",
    }
}
