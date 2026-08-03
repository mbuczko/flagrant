use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    Feature, FeatureOverride, Variant,
    payload::{FeaturePatch, SegmentVariantWeight},
};

use crate::handlers::internal::effectives as effective;

use super::Tabular;

const SHOW_OVERRIDES: usize = 3;

/// A staged change to the in-context identity's override for this feature.
pub enum IdentityPending {
    /// A new or updated override was staged (`SET override`).
    Override(String),
    /// The existing override was staged for removal (`UNSET override`).
    Unpin(String),
}

impl IdentityPending {
    fn identity_value(&self) -> &str {
        match self {
            IdentityPending::Override(v) | IdentityPending::Unpin(v) => v,
        }
    }
}

/// Context passed to `Feature::display` to show both committed and pending overrides.
pub struct OverridesContext {
    pub committed: Vec<FeatureOverride>,
    /// Identity with staged change in a context.
    pub identity_pending: Option<IdentityPending>,
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
        if selfs.is_empty() {
            println!("No features found.");
            return;
        }
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
                Layout::Expandable(40),
                Align::Left,
            )
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(30), Align::Left)
            .width(Width::Percentage(100))
            .build()
            .render(rows)
    }

    fn display(&self, patch: Option<&FeaturePatch>, ctx: &OverridesContext) {
        let title = "FEATURE".bold().to_string();
        let is_deleted = patch.is_some_and(|p| p.delete);

        let (name_str, name_stage) = if is_deleted {
            (self.name.red().to_string(), "✕ deleting".red().to_string())
        } else {
            match patch.and_then(|p| p.name.as_deref()) {
                Some(n) => (n.yellow().to_string(), "‣ updating".yellow().to_string()),
                None => (self.name.clone(), String::new()),
            }
        };

        let has_tag_ops = !is_deleted && patch.is_some_and(|p| !p.tags.is_empty());
        let tags_str = if is_deleted {
            self.tags.to_string().red().to_string()
        } else if has_tag_ops {
            let eff_tags = effective::effective_tags(self, patch);

            if eff_tags.is_empty() {
                "(none)".yellow().to_string()
            } else {
                eff_tags
                    .iter()
                    // Staged-removed tags are kept visible (not dropped) so they still show
                    // up in the list, colored red, instead of silently vanishing.
                    .map(|t| {
                        if t.is_deleted {
                            t.name.red().to_string()
                        } else if t.is_staged_add {
                            t.name.green().to_string()
                        } else {
                            t.name.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        } else {
            self.tags.to_string().blue().to_string()
        };

        let tags_stage = if is_deleted {
            "✕ deleting".red().to_string()
        } else if has_tag_ops {
            "‣ updating".yellow().to_string()
        } else {
            String::new()
        };

        let pending_enabled = (!is_deleted)
            .then(|| patch.and_then(|p| p.is_enabled))
            .flatten();
        let pending_archived = (!is_deleted)
            .then(|| patch.and_then(|p| p.is_archived))
            .flatten();

        let status = if is_deleted {
            format!(
                "● {}",
                if self.is_archived {
                    "archived"
                } else if self.is_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            )
            .red()
            .to_string()
        } else if pending_archived.unwrap_or(self.is_archived) {
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

        let status_stage = if is_deleted {
            "✕ deleting".red().to_string()
        } else if pending_enabled.is_some() || pending_archived.is_some() {
            "‣ updating".yellow().to_string()
        } else {
            String::new()
        };

        let desc_str = if is_deleted {
            self.description.red().to_string()
        } else {
            match patch.and_then(|p| p.description.as_deref()) {
                Some("") => "(cleared)".yellow().to_string(),
                Some(d) => d.yellow().to_string(),
                None => self.description.clone(),
            }
        };

        let desc_stage = if is_deleted {
            "✕ deleting".red().to_string()
        } else if patch.and_then(|p| p.description.as_ref()).is_some() {
            "‣ updating".yellow().to_string()
        } else {
            String::new()
        };

        let eff = if is_deleted {
            effective::effective_variants(self, None)
        } else {
            effective::effective_variants(self, patch)
        };
        let has_ops = !is_deleted && patch.is_some_and(|p| !p.variants.is_empty());
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
                format_weight_bar(weight, 10),
                marker,
                (i + 1).to_string().dimmed(),
                e.value
            );

            if is_deleted || e.is_deleted {
                variant_lines.push(line.red().to_string());
                variant_stage.push("✕ deleting".red().to_string());
            } else if e.is_staged_add {
                variant_lines.push(line.green().to_string());
                variant_stage.push("‣ adding".green().to_string());
            } else if e.value_modified
                || e.weight_modified
                || (e.is_control && has_ops && weight != e.weight)
            {
                variant_lines.push(line.yellow().to_string());
                let label = if e.value_modified || e.weight_modified {
                    "‣ updating"
                } else {
                    "~ adjusting"
                };
                variant_stage.push(label.yellow().to_string());
            } else {
                variant_lines.push(line);
                variant_stage.push(String::new());
            }
        }

        let variants = variant_lines.join("\n");
        let variants_stage_str = variant_stage.join("\n");

        let (overrides_lines, overrides_stages) = override_lines(&self.variants, ctx, is_deleted);
        let overrides_str = overrides_lines.join("\n");
        let overrides_stage_str = overrides_stages.join("\n");
        let overrides_has_staged = overrides_stages.iter().any(|s| !s.is_empty());

        let has_staged = !name_stage.is_empty()
            || !status_stage.is_empty()
            || !desc_stage.is_empty()
            || !tags_stage.is_empty()
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
                    Layout::Fixed(14),
                    Align::Left,
                    Overflow::Truncate,
                    variant_stage.len().max(1),
                )
                .width(Width::Percentage(100))
                .add_title_with_align(&title, TitleAlign::LeftOffset(1))
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
                .add_title_with_align(&title, TitleAlign::LeftOffset(6))
                .build()
        };

        let rows: Vec<Vec<String>> = if has_staged {
            let mut rows = vec![
                vec!["name".to_string(), name_str, name_stage],
                vec!["status".to_string(), status, status_stage],
                vec!["variants".to_string(), variants, variants_stage_str],
            ];
            if !overrides_str.is_empty() {
                rows.push(vec![
                    "overrides".to_string(),
                    overrides_str,
                    overrides_stage_str,
                ]);
            }
            rows.push(vec!["tags".to_string(), tags_str, tags_stage]);
            rows.push(vec!["description".to_string(), desc_str, desc_stage]);
            rows
        } else {
            let mut rows = vec![
                vec!["name".to_string(), name_str],
                vec!["status".to_string(), status],
                vec!["variants".to_string(), variants],
            ];
            if !overrides_str.is_empty() {
                rows.push(vec!["overridden-by".to_string(), overrides_str]);
            }
            rows.push(vec!["tags".to_string(), tags_str]);
            rows.push(vec!["description".to_string(), desc_str]);
            rows
        };
        table.render(rows);
        println!("  {} control variant\n", "★".dimmed());
    }
}

/// Builds the "overrides" row content: one line per identity-overrides group (if any) and
/// one line per segment override, each paired with its staging annotation (empty string if
/// unstaged). Returned vectors stay in lockstep - one stage entry per content line - since
/// callers join them by "\n" and render as aligned rows in adjacent table columns.
fn override_lines(
    variants: &[Variant],
    ctx: &OverridesContext,
    is_deleted: bool,
) -> (Vec<String>, Vec<String>) {
    let mut overrides_lines: Vec<String> = Vec::new();
    let mut overrides_stages: Vec<String> = Vec::new();

    if let Some((content, stage)) = identity_override_line(ctx, is_deleted) {
        overrides_lines.push(content);
        overrides_stages.push(stage);
    }

    let (segment_lines, segment_stages) = segment_override_lines(variants, ctx, is_deleted);
    overrides_lines.extend(segment_lines);
    overrides_stages.extend(segment_stages);

    (overrides_lines, overrides_stages)
}

/// Builds the single grouped "identity" overrides line - up to [`SHOW_OVERRIDES`] committed
/// identities plus a "(+N more)" suffix, with the identity carrying a pending change (if any)
/// colored to stand out from the rest - paired with its staging annotation. Returns `None`
/// when there's nothing to show (no committed identities and no pending change).
fn identity_override_line(ctx: &OverridesContext, is_deleted: bool) -> Option<(String, String)> {
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

    if committed_identities.is_empty() && (is_deleted || ctx.identity_pending.is_none()) {
        return None;
    }

    let mut identities = committed_identities
        .iter()
        .take(SHOW_OVERRIDES)
        .cloned()
        .collect::<Vec<_>>();

    if !is_deleted
        && let Some(pending) = ctx.identity_pending.as_ref()
        && !committed_identities.contains(&pending.identity_value())
    {
        identities.push(pending.identity_value())
    }

    let rest = identities.len().saturating_sub(SHOW_OVERRIDES);
    // Only the identity with a pending change is colored - dimmed if it's being
    // unpinned, green if it's a new/updated override - the rest keep their
    // default style so the pending one stands out.
    let pending_value = ctx.identity_pending.as_ref().map(|p| p.identity_value());
    let line = identities
        .iter()
        .map(|id| match &ctx.identity_pending {
            Some(IdentityPending::Unpin(_)) if pending_value == Some(*id) => {
                id.red().dimmed().to_string()
            }
            Some(IdentityPending::Override(_)) if pending_value == Some(*id) => {
                id.green().to_string()
            }
            _ => id.dimmed().to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut content = if rest > 0 {
        format!("{} › {} (+{} more)", "identity".bright_blue(), line, rest)
    } else {
        format!("{} › {}", "identity".bright_blue(), line)
    };

    if is_deleted {
        content = content.red().to_string()
    }

    let stage = if is_deleted {
        "✕ deleting".red().to_string()
    } else if let Some(pending) = &ctx.identity_pending {
        match pending {
            IdentityPending::Override(_) => "‣ updating".yellow().to_string(),
            IdentityPending::Unpin(_) => "✕ deleting".red().to_string(),
        }
    } else {
        String::new()
    };

    Some((content, stage))
}

/// Builds one line per segment override - every committed segment, plus (if not already
/// among them) a newly staged segment override not yet committed - each paired with its
/// staging annotation.
fn segment_override_lines(
    variants: &[Variant],
    ctx: &OverridesContext,
    is_deleted: bool,
) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    let mut stages: Vec<String> = Vec::new();
    let mut pending_seg_shown = false;

    for ovr in &ctx.committed {
        if let FeatureOverride::Segment { name, weights } = ovr {
            let is_current_pending = !is_deleted
                && ctx
                    .segment_pending
                    .as_ref()
                    .map(|(n, _)| n == name)
                    .unwrap_or(false);

            if is_deleted {
                let parts = segment_weights(weights, variants);
                let line = format!(
                    "{}  › {} {}",
                    "segment".bright_blue(),
                    name.dimmed(),
                    parts.join(", ")
                )
                .red()
                .to_string();

                lines.push(line);
                stages.push("✕ deleting".red().to_string());
            } else if is_current_pending {
                pending_seg_shown = true;
                let (line, stage) = match &ctx.segment_pending {
                    Some((_, Some(pending_weights))) => {
                        let parts =
                            segment_weights_with_control_remainder(pending_weights, variants);
                        let line = format!(
                            "{}  › {} {}",
                            "segment".bright_blue(),
                            name.dimmed(),
                            parts.join(", ")
                        )
                        .yellow()
                        .to_string();
                        (line, "‣ updating".yellow().to_string())
                    }
                    Some((_, None)) => {
                        let parts = segment_weights(weights, variants);
                        let line = format!(
                            "{}  › {} {}",
                            "segment".bright_blue(),
                            name.dimmed(),
                            parts.join(", ")
                        )
                        .red()
                        .to_string();
                        (line, "✕ deleting".red().to_string())
                    }
                    None => unreachable!(),
                };
                lines.push(line);
                stages.push(stage);
            } else {
                let parts = segment_weights(weights, variants);
                lines.push(format!(
                    "{}  › {} {}",
                    "segment".bright_blue(),
                    name.dimmed(),
                    parts.join(", ")
                ));
                stages.push(String::new())
            }
        }
    }

    // Pending segment set for a segment not yet in committed - show as new added line.
    if !is_deleted
        && !pending_seg_shown
        && let Some((seg_name, Some(pending_weights))) = &ctx.segment_pending
    {
        let parts = segment_weights_with_control_remainder(pending_weights, variants);
        let line = format!(
            "{}  › {} {}",
            "segment".bright_blue(),
            seg_name.dimmed(),
            parts.join(", ")
        )
        .green()
        .to_string();

        lines.push(line);
        stages.push("‣ adding".green().to_string());
    }

    (lines, stages)
}

/// Resolves a boolean flag's committed value against an optional staged one, formatting it
/// as `on` (or `off`, depending which is effective) - colored yellow if a staged value is
/// overriding the committed one, plain otherwise.
fn resolve(pending: Option<bool>, committed: bool, on: &str, off: &str) -> String {
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
}

/// Formats each weight as `"<value> → <weight>%"`, skipping any whose `variant_id` no
/// longer resolves to a known variant. Unlike [`segment_weight_parts_with_control_remainder`],
/// this doesn't synthesize a control entry - use it only for committed overrides, which
/// already carry one.
fn segment_weights(weights: &[SegmentVariantWeight], variants: &[Variant]) -> Vec<String> {
    weights
        .iter()
        .filter_map(|w| format_weight_percent(w, variants))
        .collect()
}

/// Same as [`segment_weight_parts`], but with the control variant's auto-balanced
/// remainder (see [`control_remainder`]) shown first - for a staged/pending override
/// that, unlike a committed one, doesn't already carry a control entry.
fn segment_weights_with_control_remainder(
    weights: &[SegmentVariantWeight],
    variants: &[Variant],
) -> Vec<String> {
    let control = variants.iter().find(|v| v.is_control());
    let sum: u32 = weights.iter().map(|w| w.weight as u32).sum();

    Some(SegmentVariantWeight {
        variant_id: control.unwrap().id,
        weight: 100u32.saturating_sub(sum) as u8,
    })
    .iter()
    .chain(weights.iter())
    .filter_map(|w| format_weight_percent(w, variants))
    .collect()
}

fn format_weight_percent(w: &SegmentVariantWeight, variants: &[Variant]) -> Option<String> {
    let v = variants.iter().find(|v| v.id == w.variant_id)?;
    let (_, bare) = v.value.decompose();
    let first_line = bare.lines().next().unwrap_or(bare);

    Some(format!("{} → {}%", first_line, w.weight))
}

fn format_weight_bar(weight: u8, width: u16) -> String {
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
