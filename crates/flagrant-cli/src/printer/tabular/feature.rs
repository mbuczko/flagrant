use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::{
    Feature, FeatureOverride, Variant,
    payload::{FeaturePatch, SegmentVariantWeight},
};

use crate::{handlers::internal::effectives as effective, printer::legend};

use super::Tabular;

const SHOW_OVERRIDES: usize = 3;

/// A staged change to the in-context identity's override for this feature.
pub enum IdentityPending {
    /// A new or updated override was staged (`OVERRIDE add`).
    Override(String),
    /// The existing override was staged for removal (`OVERRIDE delete`).
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
                let other_variants = feat.variants.iter().filter(|v| !v.is_control()).count();
                let value = if other_variants > 0 {
                    format!(
                        "{} {}",
                        feat.get_default_value(),
                        format!(
                            "(+{other_variants} other variant{})",
                            if other_variants == 1 { "" } else { "s" }
                        )
                        .dimmed()
                    )
                } else {
                    feat.get_default_value().to_string()
                };
                let state = if feat.is_archived {
                    format!("{} ARCH", "●".dimmed())
                } else if feat.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                [
                    feat.name.clone(),
                    state,
                    value,
                    feat.description.clone(),
                    tags,
                ]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATUS".into(), Layout::Fixed(8), Align::Left)
            .add_column_named_with_align(
                "DEFAULT VALUE".into(),
                Layout::Expandable(40),
                Align::Left,
            )
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(30), Align::Left)
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(20), Align::Left)
            .width(Width::Percentage(100))
            .build()
            .render(rows)
    }

    fn display(&self, patch: Option<&FeaturePatch>, ctx: &OverridesContext) {
        let title = "FEATURE".bold().to_string();
        let is_deleted = patch.is_some_and(|p| p.delete);

        let staged_name = patch.and_then(|p| p.name.as_deref());
        let name_modified = staged_name.is_some();
        let name_str = legend::stage_color(
            if is_deleted {
                self.name.as_str()
            } else {
                staged_name.unwrap_or(&self.name)
            },
            is_deleted,
            false,
            name_modified,
        )
        .into_owned();

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
            self.tags.to_string().white().to_string()
        };

        let pending_enabled = (!is_deleted)
            .then(|| patch.and_then(|p| p.is_enabled))
            .flatten();
        let pending_archived = (!is_deleted)
            .then(|| patch.and_then(|p| p.is_archived))
            .flatten();

        let status = if pending_archived.unwrap_or(self.is_archived) {
            format!("{} ARCHIVED", "●".dimmed())
        } else if pending_enabled.unwrap_or(self.is_enabled) {
            format!("{} ON", "●".green())
        } else {
            format!("{} OFF", "●".red())
        };

        let status_modified = pending_enabled.is_some() || pending_archived.is_some();
        let status = legend::stage_color(status, is_deleted, false, status_modified).into_owned();

        let staged_srv = (!is_deleted)
            .then(|| patch.and_then(|p| p.is_srv))
            .flatten();
        let srv_modified = staged_srv.is_some();
        let srv_effective = staged_srv.unwrap_or(self.is_srv);
        let srv_str = legend::stage_color(
            if srv_effective { "ON" } else { "OFF" },
            is_deleted,
            false,
            srv_modified,
        )
        .into_owned();

        let staged_desc = patch.and_then(|p| p.description.as_deref());
        let desc_modified = staged_desc.is_some();
        let desc_str = legend::stage_color(
            if is_deleted {
                self.description.as_str()
            } else {
                match staged_desc {
                    Some("") => "(cleared)",
                    Some(d) => d,
                    None => &self.description,
                }
            },
            is_deleted,
            false,
            desc_modified,
        )
        .into_owned();

        let eff = effective::effective_variants(self, if is_deleted { None } else { patch });
        let total_lines = eff.len();

        let has_ops = !is_deleted && patch.is_some_and(|p| !p.variants.is_empty());
        let non_control_total: u32 = eff
            .iter()
            .filter(|e| !e.is_control && !e.is_deleted)
            .map(|e| e.weight as u32)
            .sum();

        let mut variant_lines: Vec<String> = Vec::with_capacity(total_lines);
        let mut variants_staged = false;

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
            let line = format!(
                "{}{} {}{} {}",
                connector,
                format_weight_bar(weight, 10),
                if e.is_control { "★" } else { " " },
                (i + 1).to_string().dimmed(),
                e.value
            );

            if is_deleted || e.is_deleted {
                variant_lines.push(line.red().to_string());
                variants_staged = true;
            } else if e.is_staged_add {
                variant_lines.push(line.green().to_string());
                variants_staged = true;
            } else if e.value_modified
                || e.weight_modified
                || (e.is_control && has_ops && weight != e.weight)
            {
                variant_lines.push(line.yellow().to_string());
                variants_staged = true;
            } else {
                variant_lines.push(line);
            }
        }

        let variants = variant_lines.join("\n");

        let (overrides_lines, overrides_has_staged) =
            override_lines(&self.variants, ctx, is_deleted);
        let overrides_str = overrides_lines.join("\n");

        let has_staged = is_deleted
            || name_modified
            || status_modified
            || srv_modified
            || desc_modified
            || has_tag_ops
            || variants_staged
            || overrides_has_staged;

        let table = FancyTable::create(FancyTableOpts::default())
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
            .build();

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
        rows.push(vec!["server-side".to_string(), srv_str]);

        table.render(rows);
        legend::print_footer(&format!(" {} control variant", "★".dimmed()), has_staged);
    }
}

/// Builds the "overrides" row content: one line per identity-overrides group (if any) and
/// one line per segment override, plus whether *any* of those lines is currently staged.
fn override_lines(
    variants: &[Variant],
    ctx: &OverridesContext,
    is_deleted: bool,
) -> (Vec<String>, bool) {
    let mut overrides_lines: Vec<String> = Vec::new();
    let mut overrides_staged = false;

    if let Some((content, staged)) = identity_override_line(ctx, is_deleted) {
        overrides_lines.push(content);
        overrides_staged |= staged;
    }

    let (segment_lines, segment_staged) = segment_override_lines(variants, ctx, is_deleted);
    overrides_lines.extend(segment_lines);
    overrides_staged |= segment_staged;

    (overrides_lines, overrides_staged)
}

/// Builds the single grouped "identity" overrides line - up to [`SHOW_OVERRIDES`] committed
/// identities plus a "(+N more)" suffix, with the identity carrying a pending change (if any)
/// colored to stand out from the rest - paired with whether it's currently staged. Returns
/// `None` when there's nothing to show (no committed identities and no pending change).
fn identity_override_line(ctx: &OverridesContext, is_deleted: bool) -> Option<(String, bool)> {
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

    let staged = is_deleted || ctx.identity_pending.is_some();

    Some((content, staged))
}

/// Builds one line per segment override - every committed segment, plus (if not already
/// among them) a newly staged segment override not yet committed - plus whether *any* of
/// those lines is currently staged.
fn segment_override_lines(
    variants: &[Variant],
    ctx: &OverridesContext,
    is_deleted: bool,
) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    let mut staged = false;
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
                staged = true;
            } else if is_current_pending {
                pending_seg_shown = true;
                let line = match &ctx.segment_pending {
                    Some((_, Some(pending_weights))) => {
                        let parts =
                            segment_weights_with_control_remainder(pending_weights, variants);
                        format!(
                            "{}  › {} {}",
                            "segment".bright_blue(),
                            name.dimmed(),
                            parts.join(", ")
                        )
                        .yellow()
                        .to_string()
                    }
                    Some((_, None)) => {
                        let parts = segment_weights(weights, variants);
                        format!(
                            "{}  › {} {}",
                            "segment".bright_blue(),
                            name.dimmed(),
                            parts.join(", ")
                        )
                        .red()
                        .to_string()
                    }
                    None => unreachable!(),
                };
                lines.push(line);
                staged = true;
            } else {
                let parts = segment_weights(weights, variants);
                lines.push(format!(
                    "{}  › {} {}",
                    "segment".bright_blue(),
                    name.dimmed(),
                    parts.join(", ")
                ));
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
        staged = true;
    }

    (lines, staged)
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
