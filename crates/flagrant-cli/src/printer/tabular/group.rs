use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    Comparator, SegmentDriver, SegmentGroup,
    payload::{SegmentPatch, SegmentPatchOp},
};

use super::Tabular;
use super::segment::{
    UTF_BTM_CORNER, UTF_TOP_CORNER, UTF_VERT_BAR, format_comparator, format_connector,
    format_driver,
};

impl Tabular for SegmentGroup {
    type Patch = SegmentPatch;
    type Context = ();

    fn list(_: &[Self]) {}

    fn display(&self, patch: Option<&SegmentPatch>, _ctx: &()) {
        let group = self;
        let group_num = group.label.strip_prefix("group-").unwrap_or(&group.label);
        let title = format!("GROUP #{group_num}").bold().to_string();

        let is_deleted = patch.is_some_and(|p| {
            p.ops.iter().any(
                |op| matches!(op, SegmentPatchOp::DeleteGroup { label } if label == &group.label),
            )
        });

        let deleted_rule_ids: std::collections::HashSet<i32> = patch
            .map(|p| {
                p.ops
                    .iter()
                    .filter_map(|op| match op {
                        SegmentPatchOp::DeleteRule { rule_id } => Some(*rule_id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let staged_add_rules: Vec<(&SegmentDriver, &Comparator, &String)> = patch
            .map(|p| {
                p.ops
                    .iter()
                    .filter_map(|op| match op {
                        SegmentPatchOp::AddRule {
                            group_label,
                            driver,
                            comparator,
                            value,
                        } if group_label == &group.label => Some((driver, comparator, value)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let rule_value_overrides: std::collections::HashMap<i32, &str> = patch
            .map(|p| {
                p.ops
                    .iter()
                    .filter_map(|op| match op {
                        SegmentPatchOp::SetRuleValue { rule_id, value } => {
                            Some((*rule_id, value.as_str()))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let rule_comparator_overrides: std::collections::HashMap<i32, &Comparator> = patch
            .map(|p| {
                p.ops
                    .iter()
                    .filter_map(|op| match op {
                        SegmentPatchOp::SetRuleComparator {
                            rule_id,
                            comparator,
                        } => Some((*rule_id, comparator)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let sym = group
            .connector
            .as_ref()
            .map(format_connector)
            .unwrap_or("(first group)");

        let sym_colored = if is_deleted {
            sym.red().to_string()
        } else if sym.len() >= 10 {
            sym.dimmed().to_string()
        } else {
            sym.bright_cyan().to_string()
        };

        let joiner_stage = if is_deleted {
            "✕ deleting".red().to_string()
        } else {
            String::new()
        };

        let mut group_lines: Vec<String> = Vec::new();
        let mut group_stage: Vec<String> = Vec::new();

        let (frame, label_colored) = if is_deleted {
            (UTF_TOP_CORNER.dimmed(), group.label.red())
        } else {
            (UTF_TOP_CORNER.dimmed(), group.label.yellow())
        };

        let desc_part = group
            .description
            .as_deref()
            .map(|d| format!(" {} {}", "─".dimmed(), d.dimmed()))
            .unwrap_or_default();

        group_lines.push(format!("{frame} {label_colored}{desc_part}"));
        group_stage.push(if is_deleted {
            "✕ deleting".red().to_string()
        } else {
            String::new()
        });

        let all_empty = group.rules.is_empty() && staged_add_rules.is_empty();

        if all_empty {
            group_lines.push(format!(
                "{}  {}",
                UTF_VERT_BAR.dimmed(),
                "(no rules)".dimmed()
            ));
            group_stage.push(String::new());
        } else {
            let max_driver = group
                .rules
                .iter()
                .map(|r| format_driver(&r.driver).len())
                .chain(
                    staged_add_rules
                        .iter()
                        .map(|(d, _, _)| format_driver(d).len()),
                )
                .max()
                .unwrap_or(0);

            for (display_idx, r) in (1usize..).zip(group.rules.iter()) {
                let driver = format_driver(&r.driver);
                let rule_deleted = deleted_rule_ids.contains(&r.id);
                let value_modified = rule_value_overrides.contains_key(&r.id);
                let comparator_modified = rule_comparator_overrides.contains_key(&r.id);
                let effective_comparator = rule_comparator_overrides
                    .get(&r.id)
                    .copied()
                    .unwrap_or(&r.comparator);
                let effective_value = rule_value_overrides
                    .get(&r.id)
                    .copied()
                    .unwrap_or(r.value.as_str());
                let cmp = format_comparator(effective_comparator);

                let (pipe, idx_str, driver_s, cmp_s, val_s, rule_stage) =
                    if is_deleted || rule_deleted {
                        (
                            UTF_VERT_BAR.dimmed(),
                            display_idx.to_string().red(),
                            driver.red(),
                            cmp.red(),
                            r.value.red(),
                            if rule_deleted {
                                "✕ deleting".red().to_string()
                            } else {
                                String::new()
                            },
                        )
                    } else if value_modified || comparator_modified {
                        (
                            UTF_VERT_BAR.dimmed(),
                            display_idx.to_string().dimmed(),
                            driver.bright_blue(),
                            if comparator_modified {
                                cmp.yellow()
                            } else {
                                cmp.dimmed()
                            },
                            if value_modified {
                                effective_value.yellow()
                            } else {
                                effective_value.green()
                            },
                            "‣ updating".yellow().to_string(),
                        )
                    } else {
                        (
                            UTF_VERT_BAR.dimmed(),
                            display_idx.to_string().dimmed(),
                            driver.bright_blue(),
                            cmp.dimmed(),
                            r.value.green(),
                            String::new(),
                        )
                    };
                group_lines.push(format!(
                    "{pipe}  {idx_str}  {driver_s:<dw$}  {cmp_s}  {val_s}",
                    dw = max_driver,
                ));
                group_stage.push(rule_stage);
            }
            for (driver, comparator, value) in &staged_add_rules {
                group_lines.push(format!(
                    "{}  {}  {:<dw$}  {}  {}",
                    UTF_VERT_BAR.green(),
                    "+".green(),
                    format_driver(driver).bright_blue(),
                    format_comparator(comparator).dimmed(),
                    value.green(),
                    dw = max_driver,
                ));
                group_stage.push("‣ adding".green().to_string());
            }
        }

        group_lines.push(UTF_BTM_CORNER.dimmed().to_string());
        group_stage.push(String::new());

        let group_str = group_lines.join("\n");
        let group_stage_str = group_stage.join("\n");
        let has_staged = !joiner_stage.is_empty() || group_stage.iter().any(|s| !s.is_empty());
        let nlines = group_lines.len();

        if has_staged {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .add_column(
                    None,
                    Layout::Fixed(12),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(5))
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["joiner".to_string(), sym_colored, joiner_stage],
                vec!["group".to_string(), group_str, group_stage_str],
            ]);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(5))
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["joiner".to_string(), sym_colored],
                vec!["group".to_string(), group_str],
            ]);
        }
    }
}
