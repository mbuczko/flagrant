use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{
    Comparator, Environment, Feature, GroupConnector, IdentityVariant, IdentityWithTraits, Segment,
    SegmentDriver, TraitValue,
    payload::{FeaturePatch, IdentityOverridePatch, IdentityPatch, SegmentPatch},
};

use crate::handlers::internal::effectives as effective;

pub trait Tabular {
    type Patch;
    type Context;

    fn describe(&self, patch: Option<&Self::Patch>, ctx: &Self::Context);

    fn list(rows: &[Self])
    where
        Self: Sized;
}

impl Tabular for Environment {
    type Patch = ();
    type Context = ();

    fn list(selfs: &[Self]) {
        let rows: Vec<_> = selfs
            .iter()
            .map(|env| {
                [
                    env.name.clone(),
                    env.description.clone().unwrap_or_default(),
                ]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
            .rseparator(None)
            .width(100)
            .build()
            .render(rows);
    }
    fn describe(&self, _patch: Option<&()>, _ctx: &()) {
        let desc_str = self.description.as_deref().unwrap_or("");
        let title = format!("Environment: {} (ID={})", self.name, self.id);
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(6), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                1,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        table.render(vec![&["NAME", &self.name], &["DESCRIPTION", desc_str]]);
    }
}

impl Tabular for IdentityWithTraits {
    type Patch = IdentityPatch;
    type Context = Vec<IdentityVariant>;

    fn list(selfs: &[Self]) {
        let rows: Vec<_> = selfs
            .iter()
            .map(|id| {
                let traits = id
                    .traits
                    .iter()
                    .map(|t| format!("{}:{}", t.name, format_trait_value(&t.value)))
                    .collect::<Vec<_>>()
                    .join(", ");
                [id.value.clone(), traits]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("IDENTITY".into(), Layout::Fixed(40), Align::Left)
            .add_column_named_with_align("TRAITS".into(), Layout::Expandable(60), Align::Left)
            .width(100)
            .build()
            .render(rows);
    }

    fn describe(&self, patch: Option<&IdentityPatch>, ctx: &Vec<IdentityVariant>) {
        let assigned_variant: Option<String> = if ctx.is_empty() {
            None
        } else {
            Some(
                ctx.iter()
                    .map(|iv| {
                        if iv.identity_id.is_some() {
                            let pin = if iv.pinned_at.is_some() {
                                "(pinned)".red().to_string()
                            } else {
                                String::default()
                            };
                            format!(
                                "{} → {} {}",
                                iv.feature_name.bright_blue(),
                                iv.feature_value
                                    .as_ref()
                                    .map(|v| v.to_string())
                                    .unwrap_or_default(),
                                pin
                            )
                        } else {
                            format!(
                                "{} → {}",
                                iv.feature_name.bright_blue(),
                                "(not yet assigned)".dimmed()
                            )
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };

        let title = format!("Identity: {} (ID={})", self.value, self.id);
        let eff_traits = effective::effective_identity_traits(self, patch);

        let mut trait_lines: Vec<String> = Vec::new();
        let mut trait_stage: Vec<String> = Vec::new();

        for t in &eff_traits {
            let name = t.name.bright_blue().to_string();
            if t.is_deleted {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .dimmed()
                        .to_string(),
                );
                trait_stage.push("deleted".red().to_string());
            } else if t.value_modified {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .yellow()
                        .to_string(),
                );
                trait_stage.push("updated".yellow().to_string());
            } else if t.is_staged_add {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .green()
                        .to_string(),
                );
                trait_stage.push("added".green().to_string());
            } else {
                trait_lines.push(format!("{}:{}", name, format_trait_value(&t.value)));
                trait_stage.push(String::new());
            }
        }

        let traits_str = if trait_lines.is_empty() {
            "(none)".dimmed().to_string()
        } else {
            trait_lines.join("\n")
        };

        let staged_pins: &[IdentityOverridePatch] =
            patch.map(|p| p.overrides.as_slice()).unwrap_or_default();
        let staged_unpins: &[String] = patch.map(|p| p.unpins.as_slice()).unwrap_or_default();

        let has_staged_traits = !trait_stage.iter().all(|s| s.is_empty());
        let has_staged_pins = !staged_pins.is_empty() || !staged_unpins.is_empty();

        if has_staged_traits || has_staged_pins {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    11,
                )
                .add_column(None, Layout::Fixed(10), Align::Left, Overflow::Truncate, 10)
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(100)
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec![
                "TRAITS".to_string(),
                traits_str,
                trait_stage.join("\n"),
            ]];

            if let Some(value) = assigned_variant {
                rows.push(vec![
                    "VARIANTS".to_string(),
                    value.to_string(),
                    String::new(),
                ]);
            }

            if has_staged_pins {
                let (mut override_lines, mut override_stage): (Vec<String>, Vec<String>) =
                    staged_pins
                        .iter()
                        .map(|o| {
                            (
                                format!("{} → {}", o.feature_name.bright_blue(), o.variant_value)
                                    .green()
                                    .to_string(),
                                "pinning".green().to_string(),
                            )
                        })
                        .unzip();

                for feature_name in staged_unpins {
                    override_lines.push(
                        format!(
                            "{} → {}",
                            feature_name.bright_blue(),
                            "(not assigned)".red()
                        )
                        .to_string(),
                    );
                    override_stage.push("unpinning".red().to_string());
                }
                rows.push(vec![
                    "OVERRIDES".to_string(),
                    override_lines.join("\n"),
                    override_stage.join("\n"),
                ]);
            }

            table.render(rows);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    10,
                )
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(100)
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec!["TRAITS".to_string(), traits_str]];

            if let Some(value) = assigned_variant {
                rows.push(vec!["VARIANTS".to_string(), value.to_string()]);
            }
            table.render(rows);
        }
    }
}

impl Tabular for Feature {
    type Patch = FeaturePatch;
    type Context = ();

    fn list(selfs: &[Self]) {
        let rows = selfs
            .iter()
            .map(|feat| {
                let tags = feat.tags.to_string();
                let value = feat.get_default_value().to_string();
                let state = if feat.is_archived {
                    format!("{} archived", "●".dimmed())
                } else if feat.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                [feat.name.clone(), state, value, tags]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATUS".into(), Layout::Fixed(12), Align::Left)
            .add_column_named_with_align(
                "DEFAULT VALUE".into(),
                Layout::Expandable(30),
                Align::Left,
            )
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(20), Align::Left)
            .width(100)
            .build()
            .render(rows)
    }

    fn describe(&self, patch: Option<&FeaturePatch>, _ctx: &()) {
        let title = format!("Feature: {} (ID={})", self.name, self.id);
        let tags = format!("{}", self.tags.to_string().bright_blue());
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                10,
            )
            .width(100)
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        let eff = effective::effective_variants(self, patch);

        // Compute the adjusted control weight when there are pending ops that affect non-control
        // variants, mirroring the auto-adjustment done on the backend.
        let has_ops = patch.is_some_and(|p| !p.variants.is_empty());
        let non_control_total: u32 = eff
            .iter()
            .filter(|e| !e.is_control && !e.is_deleted)
            .map(|e| e.weight as u32)
            .sum();

        let total_lines = eff.len();
        let mut variant_lines: Vec<String> = Vec::with_capacity(total_lines);

        for (i, e) in eff.iter().enumerate() {
            let connector = if i + 1 == total_lines {
                "╰╴"
            } else {
                "├╴"
            };

            let weight = if e.is_control && has_ops {
                100u32.saturating_sub(non_control_total) as u8
            } else {
                e.weight
            };
            let marker = if e.is_control { "★" } else { " " };
            let line = format!("{}{} {}{}", connector, bar(weight, 10), marker, e.value);

            variant_lines.push(if e.is_deleted {
                line.dimmed().to_string()
            } else if e.is_staged_add {
                line.green().to_string()
            } else if e.value_modified
                || e.weight_modified
                || (e.is_control && has_ops && weight != e.weight)
            {
                line.yellow().to_string()
            } else {
                line
            });
        }

        let variants = variant_lines.join("\n");

        // Resolve a pending override against a committed bool, coloring the result string yellow
        // when the pending value is present, otherwise returning it as-is.
        let resolve = |pending: Option<bool>, committed: bool, on: &str, off: &str| -> String {
            let (effective, is_pending) = match pending {
                Some(v) => (v, true),
                None => (committed, false),
            };
            let s = if effective {
                on.to_string()
            } else {
                off.to_string()
            };
            if is_pending {
                s.yellow().to_string()
            } else {
                s
            }
        };

        let pending_enabled = patch.and_then(|p| p.is_enabled);
        let pending_archived = patch.and_then(|p| p.is_archived);
        let status = if pending_archived.unwrap_or(self.is_archived) {
            resolve(
                pending_archived,
                self.is_archived,
                &format!("{} archived", "●".dimmed()),
                &format!("{} active", "●".green()),
            )
        } else {
            resolve(
                pending_enabled,
                self.is_enabled,
                &format!("{} ON", "●".green()),
                &format!("{} OFF", "●".red()),
            )
        };

        table.render(vec![
            &["STATUS", &status],
            &["VARIANTS", &variants],
            &["TAGS", &tags],
        ]);
        println!("  {} control variant\n", "★".dimmed());
    }
}

/// A single row for the variant listing table.
/// All strings are pre-colored by the caller.
/// When `stage` is `None`, the STATE column is omitted entirely.
pub struct VariantRow {
    pub index: String,
    pub weight: String,
    pub value: String,
    pub stage: Option<String>,
}

/// Render a variant listing table, consuming the rows.
///
/// If any row carries a `stage`, a STAGE column is added for all rows.
pub fn variant_list(rows: Vec<VariantRow>) {
    let with_stage = rows.iter().any(|r| r.stage.is_some());

    if with_stage {
        let data: Vec<[String; 4]> = rows
            .into_iter()
            .map(|r| [r.index, r.weight, r.value, r.stage.unwrap_or_default()])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("#".into(), Layout::Fixed(4), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
            .add_column_named_with_align("STAGE".into(), Layout::Fixed(10), Align::Left)
            .width(100)
            .build()
            .render(data);
    } else {
        let data: Vec<[String; 3]> = rows
            .into_iter()
            .map(|r| [r.index, r.weight, r.value])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("#".into(), Layout::Fixed(4), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
            .width(100)
            .build()
            .render(data);
    }

    println!("  {} control variant\n", "★".dimmed());
}

pub fn bar(weight: u8, width: u16) -> String {
    let total_halves = weight as u32 * width as u32 * 2 / 100;
    let full_chars = (total_halves / 2) as usize;
    let half = total_halves % 2 == 1;
    let filled = full_chars + half as usize;

    let mut bar = String::with_capacity(width as usize);
    for _ in 0..full_chars {
        bar.push('━');
    }
    if half {
        bar.push('╸');
    }
    for _ in filled..width as usize {
        bar.push(' ');
    }
    format!("{0: <3}% {1: <10}", weight, bar)
}

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

        // Title: show staged name change inline
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

        // Description row
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

        // Build group lines and parallel stage column entries
        let mut group_lines: Vec<String> = Vec::new();
        let mut group_stage: Vec<String> = Vec::new();

        for group in &eff.groups {
            // Connector separator
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

            // Group header
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
                "deleted".red().to_string()
            } else if group.is_staged_add {
                "added".green().to_string()
            } else {
                String::new()
            });

            // Rules
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
                let max_cmp = visible_rules
                    .iter()
                    .map(|r| format_comparator(&r.comparator).len())
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
                                // Only tag individually deleted rules; group header already says "deleted"
                                if r.is_deleted {
                                    "deleted".red().to_string()
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
                                "added".green().to_string(),
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
                        "{pipe}  {idx_str}  {driver_s:<dw$}  {cmp_s:<cw$}  {val_s}",
                        dw = max_driver,
                        cw = max_cmp,
                    ));
                    group_stage.push(rule_stage);

                    if !r.is_staged_add {
                        display_idx += 1;
                    }
                }
            }

            // Closing corner
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

        // Only show stage column when there is actual content — don't show it for
        // name-only changes (reflected in title) that produce no group/rule labels.
        let has_staged = !desc_stage.is_empty() || group_stage.iter().any(|s| !s.is_empty());

        let rules_stage_str = group_stage.join("\n");

        let table = if has_staged {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(13), Align::Right, Overflow::Truncate, 1)
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
                vec!["DESCRIPTION".to_string(), desc_str, desc_stage],
                vec!["RULES".to_string(), groups_str, rules_stage_str],
            ]
        } else {
            vec![
                vec!["DESCRIPTION".to_string(), desc_str],
                vec!["RULES".to_string(), groups_str],
            ]
        };
        table.render(rows);

        // Hints below the table
        let has_visible = eff.groups.iter().any(|g| !g.is_deleted || g.is_staged_add);
        if !has_visible {
            println!("{}", "(no groups — use GROUP add to create one)".dimmed());
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

fn format_trait_value(value: &Option<TraitValue>) -> String {
    match value {
        Some(TraitValue::Str(v)) => format!("{v} ({})", "string".yellow()),
        Some(TraitValue::Int(v)) => format!("{v} ({})", "int".yellow()),
        Some(TraitValue::Float(v)) => format!("{v} ({})", "float".yellow()),
        Some(TraitValue::Bool(v)) => format!("{v} ({})", "bool".yellow()),
        None => "(unset)".dimmed().to_string(),
    }
}
