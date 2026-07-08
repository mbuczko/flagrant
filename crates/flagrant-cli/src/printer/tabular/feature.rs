use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{Feature, payload::FeaturePatch};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

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
            .width(Width::Percentage(100))
            .build()
            .render(rows)
    }

    fn describe(&self, patch: Option<&FeaturePatch>, _ctx: &()) {
        let title = format!("Feature: {} (ID={})", self.name, self.id);
        let tags = format!("{}", self.tags.to_string().bright_blue());

        let resolve = |pending: Option<bool>, committed: bool, on: &str, off: &str| -> String {
            let (effective, is_pending) = match pending {
                Some(v) => (v, true),
                None => (committed, false),
            };
            let s = if effective { on } else { off };
            if is_pending {
                s.yellow().to_string()
            } else {
                s.to_string()
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
        let status_stage = if pending_enabled.is_some() || pending_archived.is_some() {
            "▪ updated".yellow().to_string()
        } else {
            String::new()
        };

        let desc_str = match patch.and_then(|p| p.description.as_deref()) {
            Some("") => "(cleared)".yellow().to_string(),
            Some(d) => d.yellow().to_string(),
            None => self.description.clone(),
        };
        let desc_stage = if patch.and_then(|p| p.description.as_ref()).is_some() {
            "▪ updated".yellow().to_string()
        } else {
            String::new()
        };

        let eff = effective::effective_variants(self, patch);
        let has_ops = patch.is_some_and(|p| !p.variants.is_empty());
        let non_control_total: u32 = eff
            .iter()
            .filter(|e| !e.is_control && !e.is_deleted)
            .map(|e| e.weight as u32)
            .sum();

        let total_lines = eff.len();
        let mut variant_lines: Vec<String> = Vec::with_capacity(total_lines);
        let mut variant_stage: Vec<String> = Vec::with_capacity(total_lines);

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
            let line = format!(
                "{}{} {}{} │ {}",
                connector,
                bar(weight, 10),
                marker,
                (i + 1).to_string().dimmed(),
                e.value
            );

            if e.is_deleted {
                variant_lines.push(line.dimmed().to_string());
                variant_stage.push("- deleted".red().to_string());
            } else if e.is_staged_add {
                variant_lines.push(line.green().to_string());
                variant_stage.push("+ added".green().to_string());
            } else if e.value_modified
                || e.weight_modified
                || (e.is_control && has_ops && weight != e.weight)
            {
                variant_lines.push(line.yellow().to_string());
                let label = if e.value_modified || e.weight_modified {
                    "▪ modified"
                } else {
                    "▪ adjusted"
                };
                variant_stage.push(label.yellow().to_string());
            } else {
                variant_lines.push(line);
                variant_stage.push(String::new());
            }
        }

        let variants = variant_lines.join("\n");
        let variants_stage_str = variant_stage.join("\n");

        let has_staged = !status_stage.is_empty()
            || !desc_stage.is_empty()
            || variant_stage.iter().any(|s| !s.is_empty());

        let table = if has_staged {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(14), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    10,
                )
                .add_column(
                    None,
                    Layout::Fixed(12),
                    Align::Left,
                    Overflow::Truncate,
                    variant_stage.len().max(1),
                )
                .width(100)
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build()
        } else {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(14), Align::Right, Overflow::Truncate, 1)
                .add_column(
                    None,
                    Layout::Expandable(120),
                    Align::Left,
                    Overflow::Truncate,
                    10,
                )
                .width(Width::Percentage(100))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build()
        };

        let rows: Vec<Vec<String>> = if has_staged {
            vec![
                vec!["STATUS".to_string(), status, status_stage],
                vec!["VARIANTS".to_string(), variants, variants_stage_str],
                vec!["TAGS".to_string(), tags, String::new()],
                vec!["DESCRIPTION".to_string(), desc_str, desc_stage],
            ]
        } else {
            vec![
                vec!["STATUS".to_string(), status],
                vec!["VARIANTS".to_string(), variants],
                vec!["TAGS".to_string(), tags],
                vec!["DESCRIPTION".to_string(), desc_str],
            ]
        };
        table.render(rows);
        println!("  {} control variant\n", "★".dimmed());
    }
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
