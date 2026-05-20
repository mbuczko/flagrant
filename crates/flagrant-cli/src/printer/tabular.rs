use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{
    Environment, Feature, FeatureValue, IdentityWithTraits, TraitValue, Variant,
    payload::{FeaturePatch, VariantPatchOp},
};

pub trait Tabular {
    fn list(rows: &[Self])
    where
        Self: Sized;
    fn describe(&self, patch: impl Into<Option<FeaturePatch>>);
}

impl Tabular for Environment {
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
    fn describe(&self, _patch: impl Into<Option<FeaturePatch>>) {
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

    fn describe(&self, _patch: impl Into<Option<FeaturePatch>>) {
        let title = format!("{} (ID={})", self.value, self.id);
        let traits = if self.traits.is_empty() {
            "(none)".dimmed().to_string()
        } else {
            self.traits
                .iter()
                .map(|t| {
                    format!(
                        "{} → {}",
                        t.name.bright_blue(),
                        format_trait_value(&t.value)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(
                None,
                Layout::Fixed(10),
                Align::Right,
                Overflow::Truncate,
                10,
            )
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                10,
            )
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        table.render(vec![&["TRAITS", &traits]]);
    }
}

impl Tabular for Feature {
    fn list(selfs: &[Self]) {
        let rows = selfs
            .iter()
            .map(|feat| {
                let tags = feat.tags.to_string();
                let value = feat.get_default_value().to_string();
                let state = if feat.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                let status = if feat.is_active {
                    String::from("active")
                } else {
                    format!("{}", "inactive".dimmed())
                };
                [feat.name.clone(), status, state, value, tags]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATUS".into(), Layout::Fixed(10), Align::Center)
            .add_column_named_with_align("STATE".into(), Layout::Slim, Align::Center)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(30), Align::Left)
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(20), Align::Left)
            .width(100)
            .build()
            .render(rows)
    }

    fn describe(&self, patch: impl Into<Option<FeaturePatch>>) {
        let patch: Option<FeaturePatch> = patch.into();

        let title = format!("{} (ID={})", &self.name, self.id);
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

        let ops = patch
            .as_ref()
            .map(|p| p.variants.as_slice())
            .unwrap_or_default();

        let deleted_ids: std::collections::HashSet<i32> = ops
            .iter()
            .filter_map(|op| match op {
                VariantPatchOp::Delete { id } => Some(*id),
                _ => None,
            })
            .collect();

        let value_overrides: std::collections::HashMap<i32, &FeatureValue> = ops
            .iter()
            .filter_map(|op| match op {
                VariantPatchOp::SetValue { id, value } => Some((*id, value)),
                _ => None,
            })
            .collect();

        let weight_overrides: std::collections::HashMap<i32, u8> = ops
            .iter()
            .filter_map(|op| match op {
                VariantPatchOp::SetWeight { id, weight } => Some((*id, *weight)),
                _ => None,
            })
            .collect();

        let staged_adds: Vec<(&FeatureValue, u8)> = ops
            .iter()
            .filter_map(|op| match op {
                VariantPatchOp::Add { value, weight } => Some((value, *weight)),
                _ => None,
            })
            .collect();

        // Compute the adjusted control weight when there are pending ops that affect non-control
        // variants, mirroring the auto-adjustment done on the backend.
        let non_control_total: u32 = self
            .variants
            .iter()
            .filter(|v| !v.is_control() && !deleted_ids.contains(&v.id))
            .map(|v| *weight_overrides.get(&v.id).unwrap_or(&v.weight) as u32)
            .sum::<u32>()
            + staged_adds.iter().map(|(_, w)| *w as u32).sum::<u32>();

        let mut sorted_variants: Vec<&Variant> = self.variants.iter().collect();
        sorted_variants.sort_by_key(|v| std::cmp::Reverse(v.weight));

        let total_lines = sorted_variants.len() + staged_adds.len();
        let mut variant_lines: Vec<String> = Vec::with_capacity(total_lines);

        for (i, v) in sorted_variants.iter().enumerate() {
            let connector = if i + 1 == total_lines {
                "╰╴"
            } else {
                "├╴"
            };
            let is_deleted = deleted_ids.contains(&v.id);
            let new_value = value_overrides.get(&v.id).copied();
            let new_weight = weight_overrides.get(&v.id).copied();

            let weight = if v.is_control() && ops.is_empty() {
                v.weight
            } else if v.is_control() {
                100u32.saturating_sub(non_control_total) as u8
            } else {
                new_weight.unwrap_or(v.weight)
            };

            let value_str = match new_value {
                Some(nv) => nv.to_string(),
                None => v.value.to_string(),
            };

            let line = format!("{}{} {}", connector, bar(weight, 10), value_str);

            variant_lines.push(if is_deleted {
                line.dimmed().to_string()
            } else if new_value.is_some()
                || new_weight.is_some()
                || (v.is_control() && !ops.is_empty() && weight != v.weight)
            {
                line.yellow().to_string()
            } else {
                line
            });
        }

        for (i, (value, weight)) in staged_adds.iter().enumerate() {
            let connector = if sorted_variants.len() + i + 1 == total_lines {
                "╰╴"
            } else {
                "├╴"
            };
            variant_lines.push(
                format!("{}{} {}", connector, bar(*weight, 10), value)
                    .green()
                    .to_string(),
            );
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

        let pending_enabled = patch.as_ref().and_then(|p| p.is_enabled);
        let pending_active = patch.as_ref().and_then(|p| p.is_active);

        let state = resolve(
            pending_enabled,
            self.is_enabled,
            &format!("{} ON", "●".green()),
            &format!("{} OFF", "●".red()),
        );
        let status = resolve(pending_active, self.is_active, "active", "inactive");

        table.render(vec![
            &["STATUS", &status],
            &["STATE", &state],
            &["VARIANTS", &variants],
            &["TAGS", &tags],
        ]);
    }
}

/// A single row for the variant listing table.
/// All strings are pre-colored by the caller.
/// When `state` is `None`, the STATE column is omitted entirely.
pub struct VariantRow {
    pub index: String,
    pub weight: String,
    pub value: String,
    pub state: Option<String>,
}

/// Render a variant listing table, consuming the rows.
///
/// If any row carries a `state`, a STATE column is added for all rows.
pub fn variant_list(rows: Vec<VariantRow>) {
    let with_state = rows.iter().any(|r| r.state.is_some());

    if with_state {
        let data: Vec<[String; 4]> = rows
            .into_iter()
            .map(|r| [r.index, r.weight, r.value, r.state.unwrap_or_default()])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("#".into(), Layout::Fixed(4), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
            .add_column_named_with_align("STATE".into(), Layout::Fixed(10), Align::Left)
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
