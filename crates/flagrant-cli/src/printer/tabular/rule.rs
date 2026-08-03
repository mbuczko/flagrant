use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp},
};

use super::Tabular;
use super::segment::{format_comparator, format_driver};

impl Tabular for SegmentRule {
    type Patch = SegmentPatch;
    type Context = (String, usize);

    fn list(_: &[Self]) {}

    fn display(&self, patch: Option<&SegmentPatch>, ctx: &(String, usize)) {
        let rule = self;
        let (group_label, index) = ctx;
        let title = format!("RULE #{index}").bold().to_string();

        let is_deleted = patch.is_some_and(|p| {
            p.ops.iter().any(
                |op| matches!(op, SegmentPatchOp::DeleteRule { rule_id } if *rule_id == rule.id),
            )
        });

        let comparator_override = patch
            .into_iter()
            .flat_map(|p| &p.ops)
            .find_map(|op| match op {
                SegmentPatchOp::SetRuleComparator {
                    rule_id,
                    comparator,
                } if *rule_id == rule.id => Some(comparator),
                _ => None,
            });
        let value_override = patch
            .into_iter()
            .flat_map(|p| &p.ops)
            .find_map(|op| match op {
                SegmentPatchOp::SetRuleValue { rule_id, value } if *rule_id == rule.id => {
                    Some(value)
                }
                _ => None,
            });

        let effective_comparator = comparator_override.unwrap_or(&rule.comparator);
        let effective_value = value_override.map(String::as_str).unwrap_or(&rule.value);

        let (driver_s, comparator_s, value_s, driver_stage, comparator_stage, value_stage) =
            if is_deleted {
                (
                    format_driver(&rule.driver).red(),
                    format_comparator(&rule.comparator).red(),
                    rule.value.red(),
                    "✕ deleting".red().to_string(),
                    "✕ deleting".red().to_string(),
                    "✕ deleting".red().to_string(),
                )
            } else {
                (
                    format_driver(&rule.driver).bright_blue(),
                    if comparator_override.is_some() {
                        format_comparator(effective_comparator).yellow()
                    } else {
                        format_comparator(effective_comparator).dimmed()
                    },
                    if value_override.is_some() {
                        effective_value.yellow()
                    } else {
                        effective_value.green()
                    },
                    String::new(),
                    if comparator_override.is_some() {
                        "‣ updating".yellow().to_string()
                    } else {
                        String::new()
                    },
                    if value_override.is_some() {
                        "‣ updating".yellow().to_string()
                    } else {
                        String::new()
                    },
                )
            };

        let has_staged =
            !driver_stage.is_empty() || !comparator_stage.is_empty() || !value_stage.is_empty();

        if !has_staged {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Wrap,
                    10,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["group".to_string(), group_label.clone()],
                vec!["driver".to_string(), driver_s.to_string()],
                vec!["comparator".to_string(), comparator_s.to_string()],
                vec!["value".to_string(), value_s.to_string()],
            ]);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Wrap,
                    12,
                )
                .add_column(None, Layout::Fixed(14), Align::Left, Overflow::Truncate, 1)
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
                .width(Width::Percentage(100))
                .build();

            table.render(vec![
                vec!["group".to_string(), group_label.clone(), String::new()],
                vec!["driver".to_string(), driver_s.to_string(), driver_stage],
                vec![
                    "comparator".to_string(),
                    comparator_s.to_string(),
                    comparator_stage,
                ],
                vec!["value".to_string(), value_s.to_string(), value_stage],
            ]);
        }
    }
}
