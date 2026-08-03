use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};

use super::Tabular;
use super::segment::{format_comparator, format_driver};
use crate::handlers::internal::effectives::EffectiveRule;

impl Tabular for EffectiveRule {
    type Patch = ();
    type Context = (String, usize);

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, ctx: &(String, usize)) {
        let rule = self;
        let (group_label, index) = ctx;
        let title = format!("RULE #{index}").bold().to_string();

        let (driver_s, comparator_s, value_s, driver_stage, comparator_stage, value_stage) =
            if rule.is_deleted {
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
                    if rule.comparator_modified {
                        format_comparator(&rule.comparator).yellow()
                    } else {
                        format_comparator(&rule.comparator).dimmed()
                    },
                    if rule.value_modified {
                        rule.value.as_str().yellow()
                    } else {
                        rule.value.as_str().green()
                    },
                    String::new(),
                    if rule.comparator_modified {
                        "‣ updating".yellow().to_string()
                    } else {
                        String::new()
                    },
                    if rule.value_modified {
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
