use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{
    Comparator, GroupConnector, Segment, SegmentDriver, SegmentGroup, SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

impl Tabular for Segment {
    type Patch = SegmentPatch;
    type Context = ();

    fn list(selfs: &[Self]) {
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
            .width(100)
            .build()
            .render(rows);
    }

    fn describe(&self, patch: Option<&SegmentPatch>, _ctx: &()) {
        let eff = effective::effective_segment(self, patch);

        let title = if eff.name_modified {
            format!(
                "Segment: {} {} {} (ID={})",
                self.name.dimmed(),
                "→".dimmed(),
                eff.name.yellow(),
                self.id
            )
        } else {
            format!("Segment: {} (ID={})", self.name, self.id)
        };

        let desc_str = if eff.description_modified {
            eff.description
                .as_deref()
                .unwrap_or("(cleared)")
                .yellow()
                .to_string()
        } else {
            eff.description.as_deref().unwrap_or("").to_string()
        };
        let desc_stage = if eff.description_modified {
            "updated".yellow().to_string()
        } else {
            String::new()
        };

        let mut group_lines: Vec<String> = Vec::new();
        let mut group_stage: Vec<String> = Vec::new();

        for group in &eff.groups {
            if let Some(connector) = &group.connector {
                let sym = match connector {
                    GroupConnector::And => "⊕ AND",
                    GroupConnector::AndNot => "⊖ AND NOT",
                };
                let sym_colored = if group.is_staged_add {
                    sym.green().to_string()
                } else if group.is_deleted {
                    sym.dimmed().to_string()
                } else {
                    sym.bright_cyan().to_string()
                };
                group_lines.push(String::new());
                group_stage.push(String::new());
                group_lines.push(format!(" {sym_colored}"));
                group_stage.push(String::new());
                group_lines.push(String::new());
                group_stage.push(String::new());
            }

            let (frame, label_colored) = if group.is_deleted {
                (
                    "╭─".red().dimmed().to_string(),
                    group.label.red().dimmed().to_string(),
                )
            } else if group.is_staged_add {
                ("╭─".green().to_string(), group.label.green().to_string())
            } else {
                ("╭─".dimmed().to_string(), group.label.yellow().to_string())
            };
            let desc_part = group
                .description
                .as_deref()
                .map(|d| format!(" {} {}", "─".dimmed(), d.dimmed()))
                .unwrap_or_default();
            group_lines.push(format!("{frame} {label_colored}{desc_part}"));
            group_stage.push(if group.is_deleted {
                "- deleted".red().to_string()
            } else if group.is_staged_add {
                "+ added".green().to_string()
            } else {
                String::new()
            });

            let visible_rules: Vec<_> = group.rules.iter().collect();
            if visible_rules.is_empty() {
                let pipe = if group.is_deleted {
                    "│".red().dimmed().to_string()
                } else if group.is_staged_add {
                    "│".green().to_string()
                } else {
                    "│".dimmed().to_string()
                };
                group_lines.push(format!("{pipe}  {}", "(no rules)".dimmed()));
                group_stage.push(String::new());
            } else {
                let max_driver = visible_rules
                    .iter()
                    .map(|r| format_driver(&r.driver).len())
                    .max()
                    .unwrap_or(0);
                let mut display_idx = 1usize;
                for r in &visible_rules {
                    let driver = format_driver(&r.driver);
                    let cmp = format_comparator(&r.comparator);

                    let (pipe, idx_str, driver_s, cmp_s, val_s, rule_stage) =
                        if group.is_deleted || r.is_deleted {
                            (
                                "│".red().dimmed().to_string(),
                                display_idx.to_string().dimmed().to_string(),
                                driver.dimmed().to_string(),
                                cmp.dimmed().to_string(),
                                r.value.dimmed().to_string(),
                                if r.is_deleted {
                                    "- deleted".red().to_string()
                                } else {
                                    String::new()
                                },
                            )
                        } else if r.is_staged_add {
                            (
                                "│".green().to_string(),
                                "+".green().to_string(),
                                driver.bright_blue().to_string(),
                                cmp.dimmed().to_string(),
                                r.value.green().to_string(),
                                "+ added".green().to_string(),
                            )
                        } else {
                            (
                                "│".dimmed().to_string(),
                                display_idx.to_string().dimmed().to_string(),
                                driver.bright_blue().to_string(),
                                cmp.dimmed().to_string(),
                                r.value.green().to_string(),
                                String::new(),
                            )
                        };

                    group_lines.push(format!(
                        "{pipe}  {idx_str}  {driver_s:<dw$}  {cmp_s}  {val_s}",
                        dw = max_driver,
                    ));
                    group_stage.push(rule_stage);

                    if !r.is_staged_add {
                        display_idx += 1;
                    }
                }
            }

            let close = if group.is_deleted {
                "╰───".red().dimmed().to_string()
            } else if group.is_staged_add {
                "╰───".green().to_string()
            } else {
                "╰───".dimmed().to_string()
            };
            group_lines.push(close);
            group_stage.push(String::new());
        }

        let groups_str = group_lines.join("\n");
        let has_staged = !desc_stage.is_empty() || group_stage.iter().any(|s| !s.is_empty());
        let rules_stage_str = group_stage.join("\n");

        let table = if has_staged {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(14), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    20,
                )
                .add_column(
                    None,
                    Layout::Fixed(10),
                    Align::Left,
                    Overflow::Truncate,
                    group_stage.len().max(1),
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build()
        } else {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(13), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    20,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build()
        };

        let rows: Vec<Vec<String>> = if has_staged {
            vec![
                vec!["RULES".to_string(), groups_str, rules_stage_str],
                vec!["DESCRIPTION".to_string(), desc_str, desc_stage],
            ]
        } else {
            vec![
                vec!["RULES".to_string(), groups_str],
                vec!["DESCRIPTION".to_string(), desc_str],
            ]
        };
        table.render(rows);

        let has_visible = eff.groups.iter().any(|g| !g.is_deleted || g.is_staged_add);
        if !has_visible {
            println!("{}", "(no groups — use GROUP add to create one)".dimmed());
        }
    }
}

impl Tabular for SegmentGroup {
    type Patch = SegmentPatch;
    type Context = ();

    fn list(_: &[Self]) {}

    fn describe(&self, patch: Option<&SegmentPatch>, _ctx: &()) {
        let group = self;
        let title = format!("Group: {}", group.label);

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

        let joiner_str = match &group.connector {
            None => "(first group)".dimmed().to_string(),
            Some(GroupConnector::And) => {
                if is_deleted {
                    "⊕ AND".dimmed().to_string()
                } else {
                    "⊕ AND".bright_cyan().to_string()
                }
            }
            Some(GroupConnector::AndNot) => {
                if is_deleted {
                    "⊖ AND NOT".dimmed().to_string()
                } else {
                    "⊖ AND NOT".bright_cyan().to_string()
                }
            }
        };
        let joiner_stage = if is_deleted {
            "- deleted".red().to_string()
        } else {
            String::new()
        };

        let mut group_lines: Vec<String> = Vec::new();
        let mut group_stage: Vec<String> = Vec::new();

        let (frame, label_colored) = if is_deleted {
            (
                "╭─".red().dimmed().to_string(),
                group.label.red().dimmed().to_string(),
            )
        } else {
            ("╭─".dimmed().to_string(), group.label.yellow().to_string())
        };
        let desc_part = group
            .description
            .as_deref()
            .map(|d| format!(" {} {}", "─".dimmed(), d.dimmed()))
            .unwrap_or_default();
        group_lines.push(format!("{frame} {label_colored}{desc_part}"));
        group_stage.push(if is_deleted {
            "- deleted".red().to_string()
        } else {
            String::new()
        });

        let all_empty = group.rules.is_empty() && staged_add_rules.is_empty();
        if all_empty {
            let pipe = if is_deleted {
                "│".red().dimmed().to_string()
            } else {
                "│".dimmed().to_string()
            };
            group_lines.push(format!("{pipe}  {}", "(no rules)".dimmed()));
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

            let mut display_idx = 1usize;
            for r in &group.rules {
                let driver = format_driver(&r.driver);
                let cmp = format_comparator(&r.comparator);
                let rule_deleted = deleted_rule_ids.contains(&r.id);
                let (pipe, idx_str, driver_s, cmp_s, val_s, rule_stage) =
                    if is_deleted || rule_deleted {
                        (
                            "│".red().dimmed().to_string(),
                            display_idx.to_string().dimmed().to_string(),
                            driver.dimmed().to_string(),
                            cmp.dimmed().to_string(),
                            r.value.dimmed().to_string(),
                            if rule_deleted {
                                "- deleted".red().to_string()
                            } else {
                                String::new()
                            },
                        )
                    } else {
                        (
                            "│".dimmed().to_string(),
                            display_idx.to_string().dimmed().to_string(),
                            driver.bright_blue().to_string(),
                            cmp.dimmed().to_string(),
                            r.value.green().to_string(),
                            String::new(),
                        )
                    };
                group_lines.push(format!(
                    "{pipe}  {idx_str}  {driver_s:<dw$}  {cmp_s}  {val_s}",
                    dw = max_driver,
                ));
                group_stage.push(rule_stage);
                display_idx += 1;
            }
            for (driver, comparator, value) in &staged_add_rules {
                group_lines.push(format!(
                    "{}  {}  {:<dw$}  {}  {}",
                    "│".green(),
                    "+".green(),
                    format_driver(driver).bright_blue(),
                    format_comparator(comparator).dimmed(),
                    value.green(),
                    dw = max_driver,
                ));
                group_stage.push("+ added".green().to_string());
            }
        }

        let close = if is_deleted {
            "╰───".red().dimmed().to_string()
        } else {
            "╰───".dimmed().to_string()
        };
        group_lines.push(close);
        group_stage.push(String::new());

        let group_str = group_lines.join("\n");
        let group_stage_str = group_stage.join("\n");
        let has_staged = !joiner_stage.is_empty() || group_stage.iter().any(|s| !s.is_empty());
        let nlines = group_lines.len();

        if has_staged {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .add_column(
                    None,
                    Layout::Fixed(10),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build();
            table.render(vec![
                vec!["JOINER".to_string(), joiner_str, joiner_stage],
                vec!["GROUP".to_string(), group_str, group_stage_str],
            ]);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build();
            table.render(vec![
                vec!["JOINER".to_string(), joiner_str],
                vec!["GROUP".to_string(), group_str],
            ]);
        }
    }
}

impl Tabular for SegmentRule {
    type Patch = SegmentPatch;
    type Context = (String, usize);

    fn list(_: &[Self]) {}

    fn describe(&self, patch: Option<&SegmentPatch>, ctx: &(String, usize)) {
        let rule = self;
        let (group_label, index) = ctx;
        let title = format!("[{group_label}] rule #{index}");

        let is_deleted = patch.is_some_and(|p| {
            p.ops.iter().any(
                |op| matches!(op, SegmentPatchOp::DeleteRule { rule_id } if *rule_id == rule.id),
            )
        });

        let (driver_s, comparator_s, value_s, stage) = if is_deleted {
            (
                format_driver(&rule.driver).dimmed().to_string(),
                format_comparator(&rule.comparator).dimmed().to_string(),
                rule.value.dimmed().to_string(),
                "- deleted".red().to_string(),
            )
        } else {
            (
                format_driver(&rule.driver).bright_blue().to_string(),
                format_comparator(&rule.comparator).dimmed().to_string(),
                rule.value.green().to_string(),
                String::new(),
            )
        };

        if stage.is_empty() {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(12), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Wrap,
                    10,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build();
            table.render(vec![
                vec!["DRIVER".to_string(), driver_s],
                vec!["COMPARATOR".to_string(), comparator_s],
                vec!["VALUE".to_string(), value_s],
            ]);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(12), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Wrap,
                    10,
                )
                .add_column(None, Layout::Fixed(10), Align::Left, Overflow::Truncate, 1)
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build();
            table.render(vec![
                vec!["DRIVER".to_string(), driver_s, stage],
                vec!["COMPARATOR".to_string(), comparator_s, String::new()],
                vec!["VALUE".to_string(), value_s, String::new()],
            ]);
        }
    }
}

fn format_driver(driver: &SegmentDriver) -> String {
    match driver {
        SegmentDriver::Identity => "identity".to_string(),
        SegmentDriver::Trait(name) => format!("trait:{name}"),
        SegmentDriver::Environment => "environment".to_string(),
    }
}

fn format_comparator(comparator: &Comparator) -> &'static str {
    match comparator {
        Comparator::ExactlyMatches => "exactly-matches",
        Comparator::DoesNotMatch => "does-not-match",
        Comparator::Contains => "contains",
        Comparator::DoesNotContain => "does-not-contain",
        Comparator::GreaterThan => "greater-than",
        Comparator::GreaterEqualThan => "greater-equal-than",
        Comparator::LowerThan => "lower-than",
        Comparator::LowerEqualThan => "lower-equal-than",
        Comparator::In => "in",
        Comparator::NotIn => "not-in",
    }
}
