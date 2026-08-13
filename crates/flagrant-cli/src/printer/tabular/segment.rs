use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    GroupConnector, OverriddenVariant, Segment, SegmentFeatureOverride,
    payload::{SegmentPatch, SegmentPatchOp},
};

use crate::{handlers::internal::effectives as effective, printer::legend};

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
            .add_column_named_with_align("GROUPS".into(), Layout::Expandable(20), Align::Left)
            .width(Width::Percentage(100))
            .build()
            .render(rows);
    }

    fn display(&self, patch: Option<&SegmentPatch>, ctx: &SegmentContext) {
        let eff = effective::effective_segment(self, patch);
        let title = "SEGMENT".bold().to_string();

        let is_deleted =
            patch.is_some_and(|p| p.ops.iter().any(|op| matches!(op, SegmentPatchOp::Delete)));

        let name_staged = is_deleted || eff.name_modified;
        let name_str =
            legend::stage_color(eff.name, is_deleted, false, eff.name_modified).into_owned();
        let desc_staged = is_deleted || eff.description_modified;
        let desc_str = legend::stage_color(
            eff.description.unwrap_or_default(),
            is_deleted,
            false,
            eff.description_modified,
        )
        .into_owned();

        // Upper-bound capacity: each group pushes at most 3 lines for its connector, 1 for
        // the label, 1 for the closing corner, plus one per rule (the +1 also safely covers
        // the single "(no rules)" placeholder line when a group has none).
        let group_capacity: usize = eff.groups.iter().map(|g| g.rules.len() + 6).sum();
        let mut group_lines: Vec<String> = Vec::with_capacity(group_capacity);
        let mut groups_staged = false;

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
                group_lines.push(String::new());
                group_lines.push(format!(" {sym_colored}"));
                group_lines.push(String::new());
            }

            let (body_lines, body_staged) = super::group::group_body_lines(group, is_deleted);
            group_lines.extend(body_lines);
            groups_staged |= body_staged;
        }

        let mut overrides_lines: Vec<String> = Vec::new();
        let mut overrides_staged = false;

        for o in &ctx.overrides {
            let parts = overridden_variant_parts(&o.weights);
            let plain_line = format!(
                "{} › {} {}",
                "feature".bright_blue(),
                o.feature_name.dimmed(),
                parts.join(", ")
            );

            if is_deleted {
                overrides_lines.push(plain_line.red().to_string());
                overrides_staged = true;
                continue;
            }

            let pending_op = patch.into_iter().flat_map(|p| &p.ops).find(|op| {
                matches!(op,
                    SegmentPatchOp::SetFeatureOverride { feature_id, .. }
                    | SegmentPatchOp::UnsetFeatureOverride { feature_id, .. }
                    if *feature_id == o.feature_id
                )
            });

            match pending_op {
                Some(SegmentPatchOp::UnsetFeatureOverride { .. }) => {
                    overrides_lines.push(plain_line.red().to_string());
                    overrides_staged = true;
                }
                Some(SegmentPatchOp::SetFeatureOverride { .. }) => {
                    overrides_lines.push(plain_line.yellow().to_string());
                    overrides_staged = true;
                }
                _ => {
                    overrides_lines.push(plain_line);
                }
            }
        }

        let has_staged = name_staged || desc_staged || groups_staged || overrides_staged;

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
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
            .build();

        let mut rows = vec![
            vec!["name".to_string(), name_str],
            vec!["rules".to_string(), group_lines.join("\n")],
        ];
        let overrides_str = overrides_lines.join("\n");

        if !overrides_str.is_empty() {
            rows.push(vec!["overrides".to_string(), overrides_str]);
        }
        rows.push(vec!["description".to_string(), desc_str]);
        table.render(rows);

        let left = if overrides_lines.is_empty() {
            String::new()
        } else {
            format!("  {} control variant", "★".dimmed())
        };
        legend::print_footer(&left, has_staged);

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
            let marker = if w.is_control { "★" } else { "" };
            format!(
                "{marker}{} → {}",
                w.value.bare_first_line(),
                format!("{}%", w.weight).bold()
            )
        })
        .collect()
}

pub(super) fn format_connector(connector: &GroupConnector) -> &'static str {
    match connector {
        GroupConnector::And => "⊕ AND",
        GroupConnector::AndNot => "⊖ AND NOT",
    }
}
