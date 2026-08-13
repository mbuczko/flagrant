use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    IdentityVariant, IdentityWithTraits, TraitValue,
    payload::{IdentityOverridePatch, IdentityPatch},
};

use crate::{handlers::internal::effectives as effective, printer::legend};

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
                    .map(|t| trait_name_value(&t.name, &t.value).white().to_string())
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

    fn display(&self, patch: Option<&IdentityPatch>, ctx: &Vec<IdentityVariant>) {
        let title = "IDENTITY".bold().to_string();
        let is_deleted = patch.is_some_and(|p| p.delete);
        let id_staged = is_deleted;
        let id_str =
            legend::stage_color(self.value.as_str(), is_deleted, false, false).into_owned();

        let eff_traits = effective::effective_identity_traits(self, patch);

        let mut trait_lines: Vec<String> = Vec::new();
        let mut traits_staged = false;

        for t in &eff_traits {
            let name_value = trait_name_value(&t.name, &t.value);
            let type_label = trait_type_label(&t.value);

            let colored = if is_deleted || t.is_deleted {
                traits_staged = true;
                name_value.red().to_string()
            } else if t.value_modified {
                traits_staged = true;
                name_value.yellow().to_string()
            } else if t.is_staged_add {
                traits_staged = true;
                name_value.green().to_string()
            } else {
                name_value.white().to_string()
            };

            trait_lines.push(format!("{colored} {type_label}"));
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
        let mut variants_staged = false;

        for iv in ctx {
            let feature = iv.feature_name.bright_blue().to_string();

            if is_deleted {
                let value = iv
                    .feature_value
                    .as_ref()
                    .map(|v| v.bare_first_line().to_string())
                    .unwrap_or_else(|| "(no variant assigned)".to_string());

                variant_lines.push(format!("{feature} → {}", value.red()));
                variants_staged = true;
            } else if let Some(pin) = staged_pins
                .iter()
                .find(|o| o.feature_name == iv.feature_name)
            {
                // Staged override: green for a brand-new pin, yellow when replacing an
                // already-pinned variant.
                let value = pin.variant_value.bare_first_line();
                let colored_value = if iv.pinned_at.is_some() {
                    value.yellow()
                } else {
                    value.green()
                };
                variant_lines.push(format!("{feature} → {colored_value}"));
                variants_staged = true;
            } else if staged_unpins.contains(&iv.feature_name) {
                // Staged unoverride - show the currently pinned variant being removed.
                let value = iv
                    .feature_value
                    .as_ref()
                    .map(|v| v.bare_first_line().to_string())
                    .unwrap_or_else(|| "(no variant assigned)".to_string());
                variant_lines.push(format!("{feature} → {}", value.red()));
                variants_staged = true;
            } else if iv.identity_id.is_some() {
                let pin_marker = if iv.pinned_at.is_some() {
                    String::from(" ★")
                } else {
                    String::new()
                };
                variant_lines.push(format!(
                    "{feature} → {}{}",
                    iv.feature_value
                        .as_ref()
                        .map(|v| v.bare_first_line().to_string())
                        .unwrap_or_default(),
                    pin_marker
                ));
            } else {
                variant_lines.push(format!("{feature} → {}", "(no variant assigned)".dimmed()));
            }
        }

        // Staged pins for features not yet in committed state
        for o in staged_pins {
            if !ctx.iter().any(|iv| iv.feature_name == o.feature_name) {
                let value = o.variant_value.bare_first_line();
                variant_lines.push(format!(
                    "{} → {}",
                    o.feature_name.bright_blue(),
                    if is_deleted { value.red() } else { value.green() }
                ));
                variants_staged = true;
            }
        }

        let variants_str = variant_lines.join("\n");
        let has_variants = !variant_lines.is_empty();
        let has_staged = id_staged || traits_staged || variants_staged;

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                10,
            )
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(9))
            .width(Width::Percentage(100))
            .build();

        let mut rows: Vec<Vec<String>> = vec![
            vec!["id".to_string(), id_str],
            vec!["traits".to_string(), traits_str],
        ];
        if has_variants {
            rows.push(vec!["features overrides".to_string(), variants_str]);
        }
        table.render(rows);

        let has_any_override =
            ctx.iter().any(|iv| iv.pinned_at.is_some()) || !staged_pins.is_empty();

        let left = if has_any_override {
            format!(" {} variant explicitly overridden", "★".dimmed())
        } else {
            String::new()
        };
        legend::print_footer(&left, has_staged);
    }
}

/// Plain, uncolored `name=value` text - kept as a single unadorned unit so callers can wrap
/// it in exactly one color (white by default, or red/yellow/green when staged) without any
/// color/reset codes already embedded inside it fighting that wrapping.
fn trait_name_value(trait_name: &str, value: &Option<TraitValue>) -> String {
    let val = match value {
        Some(TraitValue::Str(v)) => v.to_string(),
        Some(TraitValue::Int(v)) => v.to_string(),
        Some(TraitValue::Float(v)) => v.to_string(),
        Some(TraitValue::Bool(v)) => v.to_string(),
        None => String::new(),
    };
    format!("{trait_name}={val}")
}

/// The trait value's type label (`string`/`int`/`float`/`bool`/`unset`), always dimmed -
/// built and colored independently of `trait_name_value` so a staged-change color applied
/// to the name=value part never bleeds into (or gets cut short by) this label.
fn trait_type_label(value: &Option<TraitValue>) -> colored::ColoredString {
    match value {
        Some(TraitValue::Str(_)) => "string",
        Some(TraitValue::Int(_)) => "int",
        Some(TraitValue::Float(_)) => "float",
        Some(TraitValue::Bool(_)) => "bool",
        None => "unset",
    }
    .dimmed()
}
