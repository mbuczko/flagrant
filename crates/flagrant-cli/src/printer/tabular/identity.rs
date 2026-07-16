use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
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
        if selfs.is_empty() {
            println!("No identities found.");
            return;
        }
        let rows: Vec<_> = selfs
            .iter()
            .map(|id| {
                let traits = id
                    .traits
                    .iter()
                    .map(|t| format_trait_value(&t.name, &t.value, false))
                    .collect::<Vec<_>>()
                    .join(", ");
                [id.value.clone(), traits]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("IDENTITY".into(), Layout::Fixed(40), Align::Left)
            .add_column_named_with_align("TRAITS".into(), Layout::Expandable(60), Align::Left)
            .width(Width::Percentage(100))
            .build()
            .render(rows);
    }

    fn describe(&self, patch: Option<&IdentityPatch>, ctx: &Vec<IdentityVariant>) {
        let title = format!("Identity: {} (ID={})", self.value, self.id);
        let eff_traits = effective::effective_identity_traits(self, patch);

        let mut trait_lines: Vec<String> = Vec::new();
        let mut trait_stage: Vec<String> = Vec::new();

        for t in &eff_traits {
            let name = t.name.bright_blue().to_string();
            if t.is_deleted {
                trait_lines.push(
                    format_trait_value(&name, &t.value, true)
                        .dimmed()
                        .to_string(),
                );
                trait_stage.push("▪ deleting".red().to_string());
            } else if t.value_modified {
                trait_lines.push(
                    format_trait_value(&name, &t.value, true)
                        .yellow()
                        .to_string(),
                );
                trait_stage.push("▪ updating".yellow().to_string());
            } else if t.is_staged_add {
                trait_lines.push(
                    format_trait_value(&name, &t.value, true)
                        .green()
                        .to_string(),
                );
                trait_stage.push("▪ adding".green().to_string());
            } else {
                trait_lines.push(format_trait_value(&name, &t.value, true));
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

        // Build variant lines: committed state overlaid with staged pins/unpins.
        let mut variant_lines: Vec<String> = Vec::new();
        let mut variant_stage: Vec<String> = Vec::new();

        for iv in ctx {
            let feature = iv.feature_name.bright_blue().to_string();
            if let Some(pin) = staged_pins
                .iter()
                .find(|o| o.feature_name == iv.feature_name)
            {
                // Staged override: show the new value
                variant_lines.push(format!("{feature} → {}", pin.variant_value.green()));
                variant_stage.push("▪ override".yellow().to_string());
            } else if staged_unpins.contains(&iv.feature_name) {
                // Staged unoverride
                variant_lines.push(format!("{feature} → {}", "(no variant assigned)".dimmed()));
                variant_stage.push("- override".red().to_string());
            } else if iv.identity_id.is_some() {
                let pin_marker = if iv.pinned_at.is_some() {
                    format!(" {}", "★".yellow())
                } else {
                    String::new()
                };
                variant_lines.push(format!(
                    "{feature} → {}{}",
                    iv.feature_value
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    pin_marker
                ));
                variant_stage.push(String::new());
            } else {
                variant_lines.push(format!("{feature} → {}", "(no variant assigned)".dimmed()));
                variant_stage.push(String::new());
            }
        }

        // Staged pins for features not yet in committed state
        for o in staged_pins {
            if !ctx.iter().any(|iv| iv.feature_name == o.feature_name) {
                variant_lines.push(format!(
                    "{} → {}",
                    o.feature_name.bright_blue(),
                    o.variant_value.green()
                ));
                variant_stage.push("+ override".green().to_string());
            }
        }

        let variants_str = variant_lines.join("\n");
        let variants_stage_str = variant_stage.join("\n");
        let has_variants = !variant_lines.is_empty();

        let has_staged_traits = !trait_stage.iter().all(|s| s.is_empty());
        let has_staged_variants = !variant_stage.iter().all(|s| s.is_empty());

        if has_staged_traits || has_staged_variants {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(100),
                    Align::Left,
                    Overflow::Truncate,
                    11,
                )
                .add_column(None, Layout::Fixed(14), Align::Left, Overflow::Truncate, 10)
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(Width::Percentage(100))
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec![
                "TRAITS".to_string(),
                traits_str,
                trait_stage.join("\n"),
            ]];
            if has_variants {
                rows.push(vec![
                    "VARIANTS".to_string(),
                    variants_str,
                    variants_stage_str,
                ]);
            }
            table.render(rows);
        } else {
            let table = FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    10,
                )
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .width(Width::Percentage(100))
                .build();

            let mut rows: Vec<Vec<String>> = vec![vec!["TRAITS".to_string(), traits_str]];
            if has_variants {
                rows.push(vec!["VARIANTS".to_string(), variants_str]);
            }
            table.render(rows);
        }

        let has_any_override =
            ctx.iter().any(|iv| iv.pinned_at.is_some()) || !staged_pins.is_empty();
        if has_any_override {
            println!("  {} variant explicitly overridden\n", "★".dimmed());
        }
    }
}

fn format_trait_value(trait_name: &str, value: &Option<TraitValue>, with_type: bool) -> String {
    let (type_label, val) = match value {
        Some(TraitValue::Str(v)) => ("string", v.to_string()),
        Some(TraitValue::Int(v)) => ("int", v.to_string()),
        Some(TraitValue::Float(v)) => ("float", v.to_string()),
        Some(TraitValue::Bool(v)) => ("bool", v.to_string()),
        None => ("unset", String::new()),
    };
    if with_type {
        let padded = format!("{:<6}", type_label);
        format!("{} : {trait_name}={val}", padded.dimmed())
    } else {
        format!("{}={val}", trait_name.bright_blue())
    }
}
