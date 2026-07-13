//! REPL command handlers for segment management.
//!
//! | Command                   | Handler            | Description                                                                 |
//! |---------------------------|--------------------|-----------------------------------------------------------------------------|
//! | `SEGMENT add`             | [`add`]            | Create a new segment and enter its context.                                 |
//! | `SEGMENT list`            | [`list`]           | List all segments in the current project.                                   |
//! | `SEGMENT describe`        | [`describe`]       | Print details of a segment.                                                 |
//! | `SEGMENT delete`          | [`delete`]         | Delete a segment by name.                                                   |
//! | `SEGMENT use`             | [`r#use`]          | Switch into a segment context.                                              |
//! | `SET name`                | [`set_name`]       | Stage a segment name change.                                                |
//! | `SET description`         | [`set_description`]| Stage a segment description change.                                         |
//! | `SET override`            | [`set_override`]   | Stage variant weight overrides for the current feature within this segment. |
//! | `UNSET override`          | [`unset_override`] | Remove staged weight overrides for the current feature within this segment. |
//! | `COMMIT`                  | [`commit`]         | Send staged segment changes to the API.                                     |
//! | `DISCARD`                 | [`discard`]        | Drop all staged segment changes.                                            |

use anyhow::bail;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, Segment,
    payload::{FeaturePatch, NewSegmentPayload, SegmentPatchOp, SegmentVariantWeight},
};

use crate::{
    handlers::{
        features,
        internal::{effectives as effective, index, stage},
        open_in_editor,
    },
    printer::tabular::Tabular,
};

fn fetch_segment(name: &str, session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();

    ctx.client
        .get::<Segment>(res.subpath(format!("/segments/{name}")))
}

/// Create a new segment and enter its context.
///
/// Expected args: `<name> [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client.post::<_, Segment>(
            res.subpath("/segments"),
            NewSegmentPayload {
                name: name.to_string(),
                description: args.get(2).map(|d| d.to_string()),
            },
        )?
    };
    segment.describe(None, &());

    let mut ctx = session.context.write().unwrap();
    ctx.segment = Some(segment);
    ctx.identity = None;

    Ok(())
}

/// List all segments in the current project, optionally filtered by a name substring.
///
/// An optional bare string argument is treated as a substring filter (e.g. `SEGMENT list seg`).
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    let pat = args.get(1).map(std::ops::Deref::deref).unwrap_or("");

    Segment::list(
        ctx.client
            .get::<Vec<Segment>>(res.subpath(format!("/segments?pattern={pat}")))?
            .as_ref(),
    );
    Ok(())
}

/// Describe a segment by name, or the current segment context.
///
/// Expected args: `[name]`
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        fetch_segment(name, session)?.describe(None, &());
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(segment) = &ctx.segment {
            segment.describe(ctx.segment_patch.as_ref().filter(|p| !p.is_empty()), &());
        } else {
            bail!("Not in a segment context. Use `SEGMENT use <name>` first.");
        }
    }
    Ok(())
}

/// Delete a segment by name.
///
/// Expected args: `<name>`
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = fetch_segment(name, session)?;
    {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client
            .delete(res.subpath(format!("/segments/{}", segment.id)))?;
    }

    let mut ctx = session.context.write().unwrap();
    if ctx.segment.as_ref().map(|s| s.id) == Some(segment.id) {
        ctx.segment = None;
    }
    println!("Segment '{}' deleted.", name);
    Ok(())
}

/// Stage a segment name change.
///
/// Expected args: `<name>`
pub fn set_name(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: SET name <name>"))?;
    let mut ctx = session.context.write().unwrap();
    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }
    let op = SegmentPatchOp::SetName(name.to_string());
    let patch = ctx.get_or_init_segment_patch();
    if let Some(existing) = patch
        .ops
        .iter_mut()
        .find(|o| matches!(o, SegmentPatchOp::SetName(_)))
    {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    println!("Staged: name = {name}");
    Ok(())
}

/// Stage a segment description change.
///
/// Expected args: `[description]` (omit to clear)
pub fn set_description(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let desc = args.get(1).map(|a| a.to_string());
    let mut ctx = session.context.write().unwrap();
    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }
    let op = SegmentPatchOp::SetDescription(desc.clone());
    let patch = ctx.get_or_init_segment_patch();
    if let Some(existing) = patch
        .ops
        .iter_mut()
        .find(|o| matches!(o, SegmentPatchOp::SetDescription(_)))
    {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    println!(
        "Staged: description = {}",
        desc.as_deref().unwrap_or("(cleared)")
    );
    Ok(())
}

/// Stage variant weight overrides for the current feature within this segment.
///
/// **Editor mode** (`SET override` — no args):
/// Opens an editor pre-filled with all non-control variants. Lines starting with `#`
/// are comments; each non-comment line is parsed as a weight (0–100) in display order.
///
/// **Inline mode** (`SET override <variant-index> <weight>`):
/// Updates a single variant's staged weight without touching others.
/// Missing entries are stored absent, which the server treats as weight=0.
pub fn set_override(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    // Collect everything we need under the read lock, then release before writing.
    // Returns None for variant_weights if the editor was closed without changes.
    let (feature_id, feature_name, environment_id, variant_weights) = {
        let ctx = session.context.read().unwrap();
        let feature = ctx.feature.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Not in a feature context. Use \"FEATURE use ...\" to set a context.")
        })?;
        let feature_id = feature.id;
        let environment_id = ctx.environment.id;
        let has_idx_arg = args.get(1).is_some();

        let variant_weights: Option<Vec<SegmentVariantWeight>> = if has_idx_arg {
            // Inline mode: always stages the result.
            let idx = args.get(1).unwrap().parse::<usize>()?;
            let weight = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Usage: SET override <variant-index> <weight>"))?
                .parse::<u8>()?;

            let variant_ref = index::resolve(idx, &ctx)?;
            let variant_id = match variant_ref {
                VariantRef::Committed(id) => id,
                VariantRef::Staged(_) => {
                    bail!(
                        "Segment overrides require committed variants. Commit the variant first."
                    );
                }
            };

            // Start from any already-staged weights for this feature, then update/insert this one.
            let mut weights: Vec<SegmentVariantWeight> = ctx
                .segment_patch
                .as_ref()
                .and_then(|p| {
                    p.ops.iter().find_map(|op| match op {
                        SegmentPatchOp::SetFeatureOverride {
                            feature_id: fid,
                            variant_weights: vw,
                            ..
                        } if *fid == feature_id => Some(vw.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_default();

            if let Some(entry) = weights.iter_mut().find(|w| w.variant_id == variant_id) {
                entry.weight = weight;
            } else {
                weights.push(SegmentVariantWeight { variant_id, weight });
            }
            Some(weights)
        } else {
            // Editor mode: prefer staged weights; fall back to committed weights from API.
            let current_weights: Vec<SegmentVariantWeight> = ctx
                .segment_patch
                .as_ref()
                .and_then(|p| {
                    p.ops.iter().find_map(|op| match op {
                        SegmentPatchOp::SetFeatureOverride {
                            feature_id: fid,
                            variant_weights: vw,
                            ..
                        } if *fid == feature_id => Some(vw.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| {
                    let segment_id = ctx.segment.as_ref().map(|s| s.id).unwrap_or(0);
                    let path = ctx.project_resource().subpath(format!(
                        "/segments/{segment_id}/features/{feature_id}/overrides/{environment_id}"
                    ));
                    ctx.client
                        .get::<Vec<SegmentVariantWeight>>(path)
                        .unwrap_or_default()
                });

            let content = build_segment_override_editor_content(
                feature,
                ctx.feature_patch.as_ref(),
                &current_weights,
            );
            let edited = open_in_editor(&content)?;
            let variants = effective::effective_variants(feature, ctx.feature_patch.as_ref());
            let non_control: Vec<_> = variants
                .iter()
                .filter(|v| !v.is_control && !v.is_deleted)
                .collect();
            let parsed = parse_segment_override_content(&edited, &non_control, &current_weights)?;

            // Skip staging if nothing changed.
            if weights_equal(&parsed, &current_weights) {
                None
            } else {
                Some(parsed)
            }
        };

        (
            feature_id,
            feature.name.clone(),
            environment_id,
            variant_weights,
        )
    };

    let Some(variant_weights) = variant_weights else {
        return Ok(());
    };

    // Stage under write lock: replace any existing SetFeatureOverride or UnsetFeatureOverride for this feature.
    let mut ctx = session.context.write().unwrap();
    let patch = ctx.get_or_init_segment_patch();
    patch.ops.retain(|op| {
        !matches!(op,
            SegmentPatchOp::SetFeatureOverride { feature_id: fid, .. } |
            SegmentPatchOp::UnsetFeatureOverride { feature_id: fid, .. }
            if *fid == feature_id
        )
    });
    patch.ops.push(SegmentPatchOp::SetFeatureOverride {
        feature_id,
        environment_id,
        variant_weights: variant_weights.clone(),
    });
    println!(
        "Staged: segment override for '{}' ({} variant weight(s))",
        feature_name,
        variant_weights.len()
    );
    Ok(())
}

/// Stage removal of all segment weight overrides for the current feature.
///
/// On `COMMIT` the server deletes all rows in `segment_variants` for this
/// (segment, feature, environment) combination.
pub fn unset_override(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Use \"FEATURE use ...\" to set a context.")
    })?;
    let feature_id = feature.id;
    let feature_name = feature.name.clone();
    let environment_id = ctx.environment.id;

    if ctx.segment.is_none() {
        bail!("Not in a segment context. Use \"SEGMENT use ...\" to set a context.");
    }

    let patch = ctx.get_or_init_segment_patch();
    patch.ops.retain(|op| {
        !matches!(op,
            SegmentPatchOp::SetFeatureOverride { feature_id: fid, .. } |
            SegmentPatchOp::UnsetFeatureOverride { feature_id: fid, .. }
            if *fid == feature_id
        )
    });
    patch.ops.push(SegmentPatchOp::UnsetFeatureOverride {
        feature_id,
        environment_id,
    });
    println!("Staged: unset segment override for '{feature_name}'");
    Ok(())
}

fn build_segment_override_editor_content(
    feature: &Feature,
    patch: Option<&FeaturePatch>,
    current_weights: &[SegmentVariantWeight],
) -> String {
    let variants = effective::effective_variants(feature, patch);
    let mut content = String::new();

    for (idx, ev) in (1..).zip(variants.iter().filter(|v| !v.is_control && !v.is_deleted)) {
        let staged_weight = ev.id.and_then(|id| {
            current_weights
                .iter()
                .find(|w| w.variant_id == id)
                .map(|w| w.weight)
        });
        let weight = staged_weight.unwrap_or(0);
        let staged = if ev.weight_modified || ev.is_staged_add {
            " (staged)"
        } else {
            ""
        };
        content.push_str(&format!(
            "# variant {idx} ({}%){}\n{weight}\n\n",
            ev.weight, staged
        ));
    }
    content.push_str("# default value auto-adjusts to the remainder (= 100 - sum of above)");
    content
}

fn parse_segment_override_content(
    text: &str,
    non_control: &[&effective::EffectiveVariant],
    current_weights: &[SegmentVariantWeight],
) -> anyhow::Result<Vec<SegmentVariantWeight>> {
    let weight_lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .collect();

    if weight_lines.len() != non_control.len() {
        bail!(
            "Expected {} weight(s), got {}. Each non-control variant needs one line.",
            non_control.len(),
            weight_lines.len()
        );
    }

    let mut result = Vec::with_capacity(non_control.len());
    let mut sum: u32 = 0;

    for (ev, line) in non_control.iter().zip(weight_lines.iter()) {
        let weight: u8 = line.trim().parse().map_err(|_| {
            anyhow::anyhow!("Invalid weight '{}': must be an integer 0–100", line.trim())
        })?;
        sum += weight as u32;
        if sum > 100 {
            bail!("Weights sum to more than 100.");
        }
        // Keep the variant_id from the committed variant; staged adds can't be used here.
        let variant_id = ev
            .id
            .ok_or_else(|| anyhow::anyhow!("Staged (uncommitted) variants cannot be overridden by a segment. Commit the variant first."))?;
        result.push(SegmentVariantWeight { variant_id, weight });
    }

    // Include any current_weights entries that map to variants not in non_control
    // (shouldn't normally happen, but guards against stale state).
    let known_ids: std::collections::HashSet<i32> = result.iter().map(|w| w.variant_id).collect();
    for cw in current_weights {
        if !known_ids.contains(&cw.variant_id) {
            result.push(cw.clone());
        }
    }

    Ok(result)
}

fn weights_equal(a: &[SegmentVariantWeight], b: &[SegmentVariantWeight]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut a_sorted: Vec<_> = a.iter().collect();
    let mut b_sorted: Vec<_> = b.iter().collect();
    a_sorted.sort_by_key(|w| w.variant_id);
    b_sorted.sort_by_key(|w| w.variant_id);
    a_sorted
        .iter()
        .zip(b_sorted.iter())
        .all(|(x, y)| x.variant_id == y.variant_id && x.weight == y.weight)
}

/// Commit all staged segment changes to the API.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let segment_id = ctx
        .segment
        .as_ref()
        .map(|s| s.id)
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;

    let patch = match &ctx.segment_patch {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Ok(()),
    };

    // Collected before the patch is moved into the request below — doesn't depend on
    // the server response, only on which ops we're about to send.
    let overridden_feature_ids: std::collections::HashSet<i32> = patch
        .ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::SetFeatureOverride { feature_id, .. }
            | SegmentPatchOp::UnsetFeatureOverride { feature_id, .. } => Some(*feature_id),
            _ => None,
        })
        .collect();

    let path = ctx
        .project_resource()
        .subpath(format!("/segments/{segment_id}"));
    let updated = ctx
        .client
        .patch::<_, Segment>(path, patch)
        .map_err(|err| anyhow::anyhow!("Segment commit failed: {err}"))?;

    updated.describe(None, &());
    ctx.segment_patch = None;
    ctx.segment = Some(updated);
    drop(ctx);

    // If this commit touched a feature's overrides, that feature's OVERRIDES section
    // just changed even though the feature itself has no pending patch of its own —
    // show it too, so the user doesn't have to run `FEATURE describe` separately.
    for feature_id in overridden_feature_ids {
        features::describe_by_id(feature_id, session)?;
    }

    Ok(())
}

/// Drop all staged segment changes.
pub fn discard(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.has_segment_pending() {
        ctx.discard_segment_patch();
        println!("Pending changes discarded.");
    }
    Ok(())
}

/// Enter segment context by name. Clears any active identity context.
///
/// Expected args: `<name>`
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = fetch_segment(name, session)?;
    segment.describe(None, &());

    let mut ctx: std::sync::RwLockWriteGuard<'_, Connection> = session.context.write().unwrap();
    ctx.variant_index.clear();
    ctx.identity = None;
    ctx.identity_patch = None;
    ctx.segment = Some(segment);

    Ok(())
}
