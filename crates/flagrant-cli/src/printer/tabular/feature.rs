use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    Feature, FeatureOverride, Variant,
    payload::{FeaturePatch, SegmentVariantWeight},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

const SHOW_OVERRIDES: usize = 3;

/// Context passed to `Feature::describe` to show both committed and pending overrides.
pub struct OverridesContext {
    pub committed: Vec<FeatureOverride>,
    /// Identity with staged change in a context.
    pub identity_pending: Option<String>,
    /// If the segment in context has a staged change for this feature:
    /// `(segment_name, Some(weights))` = override set; `(segment_name, None)` = unset.
    pub segment_pending: Option<(String, Option<Vec<SegmentVariantWeight>>)>,
}

impl OverridesContext {
    pub fn committed_only(committed: Vec<FeatureOverride>) -> Self {
        Self {
            committed,
            identity_pending: None,
            segment_pending: None,
        }
    }
}

impl Tabular for Feature {
    type Patch = FeaturePatch;
    type Context = OverridesContext;

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

    fn describe(&self, patch: Option<&FeaturePatch>, ctx: &OverridesContext) {
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

        // Build overrides lines and stage annotations in parallel.
        let mut overrides_lines: Vec<String> = Vec::new();
        let mut overrides_stages: Vec<String> = Vec::new();

        // Identity overrides: one grouped line with optional staging annotation.
        let committed_identities: Vec<&str> = ctx
            .committed
            .iter()
            .filter_map(|o| {
                if let FeatureOverride::Identity(v) = o {
                    Some(v.as_str())
                } else {
                    None
                }
            })
            .collect();

        if !committed_identities.is_empty() || ctx.identity_pending.is_some() {
            let mut identities = committed_identities
                .iter()
                .take(SHOW_OVERRIDES)
                .cloned()
                .collect::<Vec<_>>();

            if let Some(pending) = ctx.identity_pending.as_ref()
                && !committed_identities.contains(&pending.as_str())
            {
                identities.push(pending)
            }

            let line = identities.join(", ");
            let rest = identities.len().saturating_sub(SHOW_OVERRIDES);
            let content = if rest > 0 {
                format!("{}: {} (+{} more)", "identities".dimmed(), line, rest)
            } else {
                format!("{}: {}", "identities".dimmed(), line)
            };
            if ctx.identity_pending.is_some() {
                // TODO: Generic "modified" annotation for now.
                // Later we may distinct between different actions (like override being added/removed).
                overrides_stages.push("▪ modified".yellow().to_string());
            }
            overrides_lines.push(content);
        }

        // Segment overrides: one line per segment with optional staging annotation.
        let mut pending_seg_shown = false;

        for ovr in &ctx.committed {
            if let FeatureOverride::Segment { name, weights } = ovr {
                let is_current_pending = ctx
                    .segment_pending
                    .as_ref()
                    .map(|(n, _)| n == name)
                    .unwrap_or(false);

                if is_current_pending {
                    pending_seg_shown = true;
                    let (line, stage) = match &ctx.segment_pending {
                        Some((_, Some(pending_weights))) => {
                            let with_control =
                                with_control_remainder(pending_weights, &self.variants);
                            let parts = segment_weight_parts(&with_control, &self.variants);
                            let line = format!(
                                "{} {}: {}",
                                "(segment)".dimmed(),
                                name.dimmed(),
                                parts.join(", ")
                            )
                            .yellow()
                            .to_string();
                            (line, "▪ modified".yellow().to_string())
                        }
                        Some((_, None)) => {
                            let parts = segment_weight_parts(weights, &self.variants);
                            let line = format!(
                                "{} {}: {}",
                                "(segment)".dimmed(),
                                name.dimmed(),
                                parts.join(", ")
                            )
                            .dimmed()
                            .to_string();
                            (line, "▪ removed".red().to_string())
                        }
                        None => unreachable!(),
                    };
                    overrides_lines.push(line);
                    overrides_stages.push(stage);
                } else {
                    let parts = segment_weight_parts(weights, &self.variants);
                    overrides_lines.push(format!(
                        "{} {}: {}",
                        "(segment)".dimmed(),
                        name.dimmed(),
                        parts.join(", ")
                    ));
                    overrides_stages.push(String::new())
                }
            }
        }

        // Pending segment set for a segment not yet in committed — show as new added line.
        if !pending_seg_shown && let Some((seg_name, Some(pending_weights))) = &ctx.segment_pending
        {
            let with_control = with_control_remainder(pending_weights, &self.variants);
            let parts = segment_weight_parts(&with_control, &self.variants);
            let line = format!(
                "{} {}: {}",
                "(segment)".dimmed(),
                seg_name.green(),
                parts.join(", ")
            )
            .green()
            .to_string();
            overrides_lines.push(line);
            overrides_stages.push("▪ added".green().to_string());
        }

        let overrides_str = overrides_lines.join("\n");
        let overrides_stage_str = overrides_stages.join("\n");
        let overrides_has_staged = overrides_stages.iter().any(|s| !s.is_empty());

        let has_staged = !status_stage.is_empty()
            || !desc_stage.is_empty()
            || variant_stage.iter().any(|s| !s.is_empty())
            || overrides_has_staged;

        let table = if has_staged {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
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
                .width(Width::Percentage(100))
                .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
                .build()
        } else {
            FancyTable::create(FancyTableOpts::default())
                .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
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
            let mut rows = vec![
                vec!["STATUS".to_string(), status, status_stage],
                vec!["VARIANTS".to_string(), variants, variants_stage_str],
                vec!["TAGS".to_string(), tags, String::new()],
                vec!["DESCRIPTION".to_string(), desc_str, desc_stage],
            ];
            if !overrides_str.is_empty() {
                rows.push(vec![
                    "OVERRIDES".to_string(),
                    overrides_str,
                    overrides_stage_str,
                ]);
            }
            rows
        } else {
            let mut rows = vec![
                vec!["STATUS".to_string(), status],
                vec!["VARIANTS".to_string(), variants],
                vec!["TAGS".to_string(), tags],
                vec!["DESCRIPTION".to_string(), desc_str],
            ];
            if !overrides_str.is_empty() {
                rows.push(vec!["OVERRIDDEN-BY".to_string(), overrides_str]);
            }
            rows
        };
        table.render(rows);
        println!("  {} control variant\n", "★".dimmed());
    }
}

/// Synthesizes the control variant's remainder (100 - sum of the given weights) for a
/// staged/pending override, which only carries the explicit non-control entries the user
/// provided. Mirrors the auto-balanced control row `list_overrides_for_feature` already
/// returns for committed overrides, so pending changes show the same "where does the rest
/// go" picture before they're committed.
fn with_control_remainder(
    weights: &[SegmentVariantWeight],
    variants: &[Variant],
) -> Vec<SegmentVariantWeight> {
    let Some(control) = variants.iter().find(|v| v.is_control()) else {
        return weights.to_vec();
    };
    let sum: u32 = weights.iter().map(|w| w.weight as u32).sum();
    let mut result = vec![SegmentVariantWeight {
        variant_id: control.id,
        weight: 100u32.saturating_sub(sum) as u8,
    }];

    result.extend(weights.iter().cloned());
    result
}

fn segment_weight_parts(weights: &[SegmentVariantWeight], variants: &[Variant]) -> Vec<String> {
    weights
        .iter()
        .filter_map(|w| {
            variants.iter().find(|v| v.id == w.variant_id).map(|v| {
                let (_, bare) = v.value.decompose();
                let first_line = bare.lines().next().unwrap_or(bare);
                format!("{} → {}", first_line, format!("{}%", w.weight).bold())
            })
        })
        .collect()
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
