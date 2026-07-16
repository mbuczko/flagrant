use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    Comparator, GroupConnector, OverriddenVariant, Segment, SegmentDriver, SegmentFeatureOverride,
    SegmentGroup, SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

const UTF_VERT_BAR: &str = "│";
const UTF_TOP_CORNER: &str = "╭─";
const UTF_BTM_CORNER: &str = "╰───";

/// Context passed to `Segment::describe` to show the features this segment overrides.
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

    fn describe(&self, patch: Option<&SegmentPatch>, ctx: &SegmentContext) {
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
            eff.description.unwrap_or_default()
        };
        let desc_stage = if eff.description_modified {
            "▪ updating".yellow().to_string()
        } else {
            String::new()
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
                let sym_colored = if group.is_staged_add {
                    sym.green()
                } else if group.is_deleted {
                    sym.dimmed()
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

            let label_colored = if group.is_deleted {
                group.label.dimmed()
            } else if group.is_staged_add {
                group.label.green()
            } else {
                group.label.yellow()
            };
            let desc_part = group
                .description
                .as_deref()
                .map(|d| format!(" {} {}", "─".dimmed(), d.dimmed()))
                .unwrap_or_default();

            group_lines.push(format!(
                "{} {label_colored}{desc_part}",
                UTF_TOP_CORNER.dimmed()
            ));
            group_stage.push(if group.is_deleted {
                "▪ deleting".red().to_string()
            } else if group.is_staged_add {
                "▪ adding".green().to_string()
            } else {
                String::new()
            });

            let visible_rules: Vec<_> = group.rules.iter().collect();
            if visible_rules.is_empty() {
                group_lines.push(format!(
                    "{}  {}",
                    UTF_VERT_BAR.dimmed(),
                    "(no rules)".dimmed()
                ));
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

                    let (idx_str, driver_s, val_s, rule_stage) = if group.is_deleted || r.is_deleted
                    {
                        (
                            display_idx.to_string().dimmed(),
                            driver.dimmed(),
                            r.value.dimmed(),
                            if r.is_deleted {
                                "▪ deleting".red().to_string()
                            } else {
                                String::new()
                            },
                        )
                    } else if r.is_staged_add {
                        (
                            "+".green(),
                            driver.bright_blue(),
                            r.value.green(),
                            "▪ adding".green().to_string(),
                        )
                    } else {
                        (
                            display_idx.to_string().dimmed(),
                            driver.bright_blue(),
                            r.value.green(),
                            String::new(),
                        )
                    };

                    group_lines.push(format!(
                        "{pipe}  {idx_str}  {driver_s:<dw$}  {cmp}  {val_s}",
                        pipe = UTF_VERT_BAR.dimmed(),
                        cmp = cmp.dimmed(),
                        dw = max_driver,
                    ));
                    group_stage.push(rule_stage);

                    if !r.is_staged_add {
                        display_idx += 1;
                    }
                }
            }

            group_lines.push(UTF_BTM_CORNER.dimmed().to_string());
            group_stage.push(String::new());
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
                    overrides_lines.push(plain_line.dimmed().to_string());
                    overrides_stages.push("▪ removing".red().to_string());
                }
                Some(SegmentPatchOp::SetFeatureOverride { .. }) if o.weights.is_empty() => {
                    // Newly staged override for a feature with no committed weights yet.
                    let line =
                        format!("{} › {} (pending)", "feature".bright_blue(), o.feature_name);

                    overrides_lines.push(line.green().to_string());
                    overrides_stages.push("▪ adding".green().to_string());
                }
                Some(SegmentPatchOp::SetFeatureOverride { .. }) => {
                    overrides_lines.push(plain_line.yellow().to_string());
                    overrides_stages.push("▪ updating".yellow().to_string());
                }
                _ => {
                    overrides_lines.push(plain_line);
                    overrides_stages.push(String::new());
                }
            }
        }

        let has_staged_overrides = overrides_stages.iter().any(|s| !s.is_empty());
        let has_staged = !desc_stage.is_empty()
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
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
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
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(Width::Percentage(100))
                .build()
        };

        let mut rows = vec![vec![
            "RULES".to_string(),
            group_lines.join("\n"),
            group_stage.join("\n"),
        ]];
        if let overrides_str = overrides_lines.join("\n")
            && !overrides_str.is_empty()
        {
            rows.push(vec![
                "OVERRIDES".to_string(),
                overrides_str,
                overrides_stages.join("\n"),
            ]);
        }
        rows.push(vec!["DESCRIPTION".to_string(), desc_str, desc_stage]);

        // The table itself only has a stage column when has_staged, so drop it from each
        // row to match - it would be all empty strings anyway in that case.
        if !has_staged {
            for row in &mut rows {
                row.truncate(2);
            }
        }
        table.render(rows);

        if !(eff.groups.iter().any(|g| !g.is_deleted || g.is_staged_add)) {
            println!(
                "{}",
                "(no group added yet - use `GROUP add ...` to create one)".dimmed()
            );
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

        let sym = group
            .connector
            .as_ref()
            .map(format_connector)
            .unwrap_or("(first group)");

        let sym_colored = if is_deleted || sym.len() >= 10 {
            sym.dimmed().to_string()
        } else {
            sym.bright_cyan().to_string()
        };

        let joiner_stage = if is_deleted {
            "▪ deleting".red().to_string()
        } else {
            String::new()
        };

        let mut group_lines: Vec<String> = Vec::new();
        let mut group_stage: Vec<String> = Vec::new();

        let (frame, label_colored) = if is_deleted {
            (UTF_TOP_CORNER.dimmed(), group.label.dimmed())
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
            "▪ deleting".red().to_string()
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
                let cmp = format_comparator(&r.comparator);
                let rule_deleted = deleted_rule_ids.contains(&r.id);
                let (pipe, idx_str, driver_s, cmp_s, val_s, rule_stage) =
                    if is_deleted || rule_deleted {
                        (
                            UTF_VERT_BAR.dimmed(),
                            display_idx.to_string().dimmed(),
                            driver.dimmed(),
                            cmp.dimmed(),
                            r.value.dimmed(),
                            if rule_deleted {
                                "▪ deleting".red().to_string()
                            } else {
                                String::new()
                            },
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
                group_stage.push("▪ adding".green().to_string());
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
                    Layout::Fixed(12),
                    Align::Left,
                    Overflow::Truncate,
                    nlines,
                )
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["JOINER".to_string(), sym_colored, joiner_stage],
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
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["JOINER".to_string(), sym_colored],
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
                format_driver(&rule.driver).dimmed(),
                format_comparator(&rule.comparator).dimmed(),
                rule.value.dimmed(),
                "▪ deleting".red().to_string(),
            )
        } else {
            (
                format_driver(&rule.driver).bright_blue(),
                format_comparator(&rule.comparator).dimmed(),
                rule.value.green(),
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
                .width(Width::Percentage(100))
                .build();
            table.render(vec![
                vec!["DRIVER".to_string(), driver_s.to_string()],
                vec!["COMPARATOR".to_string(), comparator_s.to_string()],
                vec!["VALUE".to_string(), value_s.to_string()],
            ]);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(12), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Wrap,
                    12,
                )
                .add_column(None, Layout::Fixed(10), Align::Left, Overflow::Truncate, 1)
                .hseparator(Some(fancy_table::Separator::Custom('-')))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(Width::Percentage(100))
                .build();

            table.render(vec![
                vec!["DRIVER".to_string(), driver_s.to_string(), stage],
                vec![
                    "COMPARATOR".to_string(),
                    comparator_s.to_string(),
                    String::new(),
                ],
                vec!["VALUE".to_string(), value_s.to_string(), String::new()],
            ]);
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

fn format_connector(connector: &GroupConnector) -> &'static str {
    match connector {
        GroupConnector::And => "⊕ AND",
        GroupConnector::AndNot => "⊖ AND NOT",
    }
}
