use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{
    IdentityVariant, IdentityWithTraits, TraitValue,
    payload::{IdentityOverridePatch, IdentityPatch},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

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
                trait_stage.push("- deleted".red().to_string());
            } else if t.value_modified {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .yellow()
                        .to_string(),
                );
                trait_stage.push("▪ updated".yellow().to_string());
            } else if t.is_staged_add {
                trait_lines.push(
                    format!("{}:{}", name, format_trait_value(&t.value))
                        .green()
                        .to_string(),
                );
                trait_stage.push("+ added".green().to_string());
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
                .add_column(None, Layout::Expandable(100), Align::Left, Overflow::Truncate, 11)
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
                rows.push(vec!["VARIANTS".to_string(), value.to_string(), String::new()]);
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
                        format!("{} → {}", feature_name.bright_blue(), "(not assigned)".red())
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
                .add_column(None, Layout::Expandable(120), Align::Left, Overflow::Truncate, 10)
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

fn format_trait_value(value: &Option<TraitValue>) -> String {
    match value {
        Some(TraitValue::Str(v)) => format!("{v} ({})", "string".yellow()),
        Some(TraitValue::Int(v)) => format!("{v} ({})", "int".yellow()),
        Some(TraitValue::Float(v)) => format!("{v} ({})", "float".yellow()),
        Some(TraitValue::Bool(v)) => format!("{v} ({})", "bool".yellow()),
        None => "(unset)".dimmed().to_string(),
    }
}
