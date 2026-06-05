use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{
    Environment, Feature, IdentityWithTraits, TraitValue,
    payload::{FeaturePatch, IdentityOverridePatch, IdentityPatch, TraitPatchOp},
};

use crate::handlers::internal::effectives as effective;

pub trait Tabular {
    type Patch;

    fn list(rows: &[Self])
    where
        Self: Sized;
    fn describe(&self, patch: Option<&Self::Patch>);
}

impl Tabular for Environment {
    type Patch = ();

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
    fn describe(&self, _patch: Option<&()>) {
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

    fn describe(&self, patch: Option<&IdentityPatch>) {
        self.describe_with_variant(patch, None);
    }
}

/// Extension trait for describing an identity with a committed variant assignment.
pub trait DescribeWithVariant {
    /// Like [`Tabular::describe`] but also renders a VARIANT row showing the current assignment.
    ///
    /// - `assigned_variant = None` — identity not yet assigned to any variant.
    /// - `assigned_variant = Some(value)` — assigned to the variant with this value.
    fn describe_with_variant(&self, patch: Option<&IdentityPatch>, assigned_variant: Option<&str>);
}

impl DescribeWithVariant for IdentityWithTraits {
    fn describe_with_variant(&self, patch: Option<&IdentityPatch>, assigned_variant: Option<&str>) {
        let title = format!("Identity: {} (ID={})", self.value, self.id);

        let ops = patch.map(|p| p.traits.as_slice()).unwrap_or_default();

        let deleted: std::collections::HashSet<&str> = ops
            .iter()
            .filter_map(|op| match op {
                TraitPatchOp::Delete { name } => Some(name.as_str()),
                _ => None,
            })
            .collect();

        let modified: std::collections::HashMap<&str, &Option<TraitValue>> = ops
            .iter()
            .filter_map(|op| match op {
                TraitPatchOp::SetValue { name, value } => Some((name.as_str(), value)),
                _ => None,
            })
            .collect();

        let added: Vec<(&str, &Option<TraitValue>)> = ops
            .iter()
            .filter_map(|op| match op {
                TraitPatchOp::Add { name, value } => Some((name.as_str(), value)),
                _ => None,
            })
            .collect();

        let mut trait_lines: Vec<String> = Vec::new();
        let mut trait_stage: Vec<String> = Vec::new();

        for t in &self.traits {
            let name = t.name.bright_blue().to_string();
            if deleted.contains(t.name.as_str()) {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .dimmed()
                        .to_string(),
                );
                trait_stage.push("deleted".red().to_string());
            } else if let Some(new_val) = modified.get(t.name.as_str()) {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(new_val))
                        .yellow()
                        .to_string(),
                );
                trait_stage.push("updated".yellow().to_string());
            } else {
                trait_lines.push(format!("{}:{}", name, format_trait_value(&t.value)));
                trait_stage.push(String::new());
            }
        }

        for (name, value) in &added {
            trait_lines.push(
                format!("{}:{}", name.bright_blue(), format_trait_value(value))
                    .green()
                    .to_string(),
            );
            trait_stage.push("added".green().to_string());
        }

        let traits_str = if trait_lines.is_empty() {
            "(none)".dimmed().to_string()
        } else {
            trait_lines.join("\n")
        };

        let staged_overrides: &[IdentityOverridePatch] =
            patch.map(|p| p.overrides.as_slice()).unwrap_or_default();

        let has_staged_traits = !trait_stage.iter().all(|s| s.is_empty());
        let has_staged_overrides = !staged_overrides.is_empty();

        if has_staged_traits || has_staged_overrides {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    10,
                )
                .add_column(None, Layout::Fixed(10), Align::Left, Overflow::Truncate, 10)
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec![
                "TRAITS".to_string(),
                traits_str,
                trait_stage.join("\n"),
            ]];

            if let Some(value) = assigned_variant {
                rows.push(vec![
                    "VARIANT".to_string(),
                    value.to_string(),
                    String::new(),
                ]);
            }

            if has_staged_overrides {
                let override_lines = staged_overrides
                    .iter()
                    .map(|o| {
                        format!("{} → {}", o.feature_name.bright_blue(), o.variant_value)
                            .green()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let override_stage = staged_overrides
                    .iter()
                    .map(|_| "staged".green().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                rows.push(vec![
                    "OVERRIDES".to_string(),
                    override_lines,
                    override_stage,
                ]);
            }

            table.render(
                rows.iter()
                    .map(|r| r.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
            );
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
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec!["TRAITS".to_string(), traits_str]];

            if let Some(value) = assigned_variant {
                rows.push(vec!["VARIANT".to_string(), value.to_string()]);
            }

            table.render(
                rows.iter()
                    .map(|r| r.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
            );
        }
    }
}

impl Tabular for Feature {
    type Patch = FeaturePatch;

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
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(30), Align::Left)
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(20), Align::Left)
            .width(100)
            .build()
            .render(rows)
    }

    fn describe(&self, patch: Option<&FeaturePatch>) {
        let title = format!("Feature: {} (ID={})", &self.name, self.id);
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
        let has_ops = patch.map_or(false, |p| !p.variants.is_empty());
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
            let line = format!("{}{} {}", connector, bar(weight, 10), e.value);

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

fn format_trait_value(value: &Option<TraitValue>) -> String {
    match value {
        Some(TraitValue::Str(v)) => format!("{v} ({})", "string".yellow()),
        Some(TraitValue::Int(v)) => format!("{v} ({})", "int".yellow()),
        Some(TraitValue::Float(v)) => format!("{v} ({})", "float".yellow()),
        Some(TraitValue::Bool(v)) => format!("{v} ({})", "bool".yellow()),
        None => "(unset)".dimmed().to_string(),
    }
}
