use std::borrow::Cow;

use anyhow::bail;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, OverriddenVariant, Segment, SegmentFeatureOverride,
    payload::{FeaturePatch, NewSegmentPayload, SegmentPatchOp, SegmentVariantWeight},
};

use crate::{
    handlers::{
        internal::{effectives as effective, index, stage},
        open_in_editor,
    },
    printer::{
        menu,
        tabular::{Tabular, segment::SegmentContext},
    },
};

fn fetch_segment(name: &str, session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();

    ctx.client
        .get::<Segment>(res.subpath(format!("/segments/{name}")))
}

/// Resolves a staged `SetFeatureOverride`'s weights into fully-detailed `OverriddenVariant`s
/// (with values and the control variant's auto-balanced remainder), so `SEGMENT show`
/// can preview the pending state instead of the stale committed weights it's replacing.
fn resolve_staged_weights(
    feature: &Feature,
    feature_patch: Option<&FeaturePatch>,
    staged: &[SegmentVariantWeight],
) -> Vec<OverriddenVariant> {
    let variants = effective::effective_variants(feature, feature_patch);
    let non_control_total: u32 = staged.iter().map(|w| w.weight as u32).sum();

    variants
        .into_iter()
        .filter(|v| !v.is_deleted)
        .filter_map(|v| {
            let variant_id = v.id?;
            if v.is_control {
                Some(OverriddenVariant {
                    variant_id,
                    value: v.value,
                    is_control: true,
                    weight: 100u32.saturating_sub(non_control_total) as u8,
                })
            } else {
                let weight = staged.iter().find(|w| w.variant_id == variant_id)?.weight;
                Some(OverriddenVariant {
                    variant_id,
                    value: v.value,
                    is_control: false,
                    weight,
                })
            }
        })
        .collect()
}

/// Fetches the features this segment overrides in the current environment.
pub(crate) fn fetch_overridden_features(
    segment_id: i32,
    session: &Session<Connection>,
) -> Vec<SegmentFeatureOverride> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    let environment_id = ctx.environment.id;

    ctx.client
        .get::<Vec<SegmentFeatureOverride>>(
            res.subpath(format!("/segments/{segment_id}/overrides/{environment_id}")),
        )
        .unwrap_or_default()
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

    // A brand-new segment can't override anything yet.
    segment.display(None, &SegmentContext::default());

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

/// Show a segment by name, or the current segment context.
///
/// Expected args: `[name]`
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = match args.get(1) {
        Some(name) => fetch_segment(name, session)?,
        None => {
            let ctx = session.context.read().unwrap();
            ctx.segment.clone().ok_or_else(|| {
                anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first.")
            })?
        }
    };

    let mut overrides = fetch_overridden_features(segment.id, session);

    // Nothing mutates the session between the fetch above and the read below - just a
    // read-only HTTP fetch - so it's safe to defer reading `segment_patch` to here rather
    // than cloning it across the gap.
    let ctx = session.context.read().unwrap();
    let is_in_context = ctx.segment.as_ref().is_some_and(|s| s.id == segment.id);
    let patch = is_in_context
        .then(|| ctx.segment_patch.as_ref())
        .flatten()
        .filter(|p| !p.is_empty());

    // `OVERRIDE add` requires a feature+segment context and switching feature is
    // blocked while this patch is pending, so the in-context feature is guaranteed to
    // be the one any `SetFeatureOverride` op refers to - at most one can be staged.
    if is_in_context && let Some(feature) = ctx.feature.as_ref() {
        let feature_id = feature.id;
        let staged_weights = patch.iter().flat_map(|p| &p.ops).find_map(|op| match op {
            SegmentPatchOp::SetFeatureOverride {
                feature_id: fid,
                variant_weights,
                ..
            } if *fid == feature_id => Some(variant_weights.as_slice()),
            _ => None,
        });

        if let Some(staged_weights) = staged_weights {
            // Resolve the actual staged weights (control's auto-balanced remainder
            // included) so the preview shows real numbers rather than a placeholder,
            // whether this replaces an existing committed override or is brand new.
            let weights =
                resolve_staged_weights(feature, ctx.feature_patch.as_ref(), staged_weights);
            if let Some(entry) = overrides.iter_mut().find(|o| o.feature_id == feature_id) {
                entry.weights = weights;
            } else {
                // A brand new override (not yet committed) won't appear in `overrides`
                // at all - add it with the resolved staged weights, rather than
                // silently omitting it until COMMIT.
                overrides.push(SegmentFeatureOverride {
                    feature_id,
                    feature_name: feature.name.clone(),
                    weights,
                });
            }
        }
    }

    segment.display(patch, &SegmentContext { overrides });
    Ok(())
}

/// Stage deletion of a segment by name.
///
/// Expected args: `<name>`
///
/// Switches into the named segment's context first if not already there (same as
/// `USE +<segment>`, failing if there are uncommitted staged changes elsewhere), then stages its
/// deletion. Nothing is sent to the API until `COMMIT`; `DISCARD` un-stages it. Once staged,
/// any other pending change for this segment is ignored by the server on commit.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };

    let already_in_context = session
        .context
        .read()
        .unwrap()
        .segment
        .as_ref()
        .is_some_and(|s| s.name == name.as_ref());

    if !already_in_context {
        stage::ensure_no_pending(session)?;
        let segment = fetch_segment(name, session)?;

        let mut ctx = session.context.write().unwrap();
        ctx.variant_index.clear();
        ctx.identity = None;
        ctx.identity_patch = None;
        ctx.segment = Some(segment);
    }

    let mut ctx = session.context.write().unwrap();
    let pending = ctx.get_or_init_segment_patch();
    if !pending
        .ops
        .iter()
        .any(|op| matches!(op, SegmentPatchOp::Delete))
    {
        pending.ops.push(SegmentPatchOp::Delete);
    }
    println!(
        "Staged: segment '{name}' marked for deletion. Run COMMIT to apply or DISCARD to cancel."
    );
    Ok(())
}

/// Stage a segment name change.
///
/// Expected args: `[name]`
///
/// If omitted, opens `$EDITOR` pre-filled with the segment's current (or already-staged)
/// name so it can be edited interactively. Unlike the description, the name can't be
/// cleared - an empty result is rejected.
pub fn rename(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }

    let name = match args.get(1) {
        Some(n) => n.to_string(),
        None => {
            let current: &str = ctx
                .segment_patch
                .as_ref()
                .and_then(|p| {
                    p.ops.iter().find_map(|op| match op {
                        SegmentPatchOp::SetName(n) => Some(n.as_str()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| ctx.segment.as_ref().unwrap().name.as_str());

            let edited = open_in_editor(current)?;
            if edited == current {
                println!("No changes made.");
                return Ok(());
            }
            edited
        }
    };

    if name.is_empty() {
        bail!("No name provided.");
    }
    println!("Staged: name = {name}");

    let op = SegmentPatchOp::SetName(name);
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
    Ok(())
}

/// Stage a segment description change.
///
/// Expected args: `[description]`
///
/// If omitted, opens `$EDITOR` pre-filled with the segment's current (or already-staged)
/// description so it can be edited interactively; leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }

    let desc = match args.get(1) {
        Some(d) => Some(d.to_string()),
        None => {
            let current: Option<&str> = ctx
                .segment_patch
                .as_ref()
                .and_then(|p| {
                    p.ops.iter().find_map(|op| match op {
                        SegmentPatchOp::SetDescription(d) => Some(d.as_deref()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| ctx.segment.as_ref().unwrap().description.as_deref());

            let edited = open_in_editor(current.unwrap_or(""))?;
            let new_desc = (!edited.is_empty()).then_some(edited);

            if new_desc.as_deref() == current {
                println!("No changes made.");
                return Ok(());
            }
            new_desc
        }
    };

    println!(
        "Staged: description = {}",
        desc.as_deref().unwrap_or_default()
    );

    let op = SegmentPatchOp::SetDescription(desc);
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
    Ok(())
}

/// Returns the weights currently in effect for `feature_id`: staged ones if a
/// `SetFeatureOverride` for it is already pending (borrowed - no clone needed), otherwise
/// the committed weights fetched from the API (owned, empty if none exist yet).
fn current_weights_for<'a>(
    ctx: &'a Connection,
    feature_id: i32,
    environment_id: i32,
) -> Cow<'a, [SegmentVariantWeight]> {
    let weights = ctx.segment_patch.as_ref().and_then(|p| {
        p.ops.iter().find_map(|op| match op {
            SegmentPatchOp::SetFeatureOverride {
                feature_id: fid,
                variant_weights,
                ..
            } if *fid == feature_id => Some(Cow::Borrowed(variant_weights.as_slice())),
            _ => None,
        })
    });

    weights.unwrap_or_else(|| {
        let segment_id = ctx.segment.as_ref().map(|s| s.id).unwrap_or(0);
        let path = ctx.project_resource().subpath(format!(
            "/segments/{segment_id}/features/{feature_id}/overrides/{environment_id}"
        ));
        Cow::Owned(
            ctx.client
                .get::<Vec<SegmentVariantWeight>>(path)
                .unwrap_or_default(),
        )
    })
}

/// Stage variant weight overrides for the current feature within this segment.
///
/// **Menu mode** (`OVERRIDE add` - no args):
/// Opens an interactive menu listing every non-control variant's current weight and
/// allows adjusting the highlighted one by 5 (clamped so the total never exceeds 100).
///
/// **Inline mode** (`OVERRIDE add <variant-index> <weight>`):
/// Updates a single variant's staged weight without touching others.
///
/// Either way, every non-control variant ends up with an explicit entry (0 for any not
/// touched) - so `FEATURE show`'s OVERRIDES row always shows the full picture, since
/// only variants with a stored weight row are displayed.
pub fn set_override(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let feature = ctx.feature.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Not in a feature context. Use \"USE <feature>\" to set a context.")
    })?;

    let environment_id = ctx.environment.id;
    let feature_id = feature.id;
    let feature_name = feature.name.clone();

    let has_idx_arg = args.get(1).is_some();
    let current_weights = current_weights_for(&ctx, feature_id, environment_id);

    let variant_weights: Option<Vec<SegmentVariantWeight>> = if has_idx_arg {
        // Inline mode: always stages the result.
        let idx = args.get(1).unwrap().parse::<usize>()?;
        let weight = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("Usage: OVERRIDE add <variant-index> <weight>"))?
            .parse::<u8>()?;

        let variant_ref = index::resolve(idx, &ctx)?;
        let variant_id = match variant_ref {
            VariantRef::Committed(id) => id,
            VariantRef::Staged(_) => {
                bail!("Segment overrides require committed variants. Commit the variant first.");
            }
        };

        if feature
            .variants
            .iter()
            .any(|v| v.id == variant_id && v.is_control())
        {
            bail!(
                "Index {idx} is the control variant - segment overrides only apply to \
                 non-control variants."
            );
        }

        // Every non-control variant gets an explicit entry (0 if not already weighted),
        // then the targeted one is updated - so untouched variants still show up in
        // FEATURE show's OVERRIDEN-BY row instead of being silently absent.
        let variants = effective::effective_variants(feature, ctx.feature_patch.as_ref());
        let mut weights: Vec<SegmentVariantWeight> = variants
            .iter()
            .filter(|v| !v.is_control && !v.is_deleted)
            .filter_map(|v| v.id)
            .map(|id| SegmentVariantWeight {
                variant_id: id,
                weight: current_weights
                    .iter()
                    .find(|w| w.variant_id == id)
                    .map(|w| w.weight)
                    .unwrap_or(0),
            })
            .collect();

        if let Some(entry) = weights.iter_mut().find(|w| w.variant_id == variant_id) {
            entry.weight = weight;
        } else {
            weights.push(SegmentVariantWeight { variant_id, weight });
        }
        Some(weights)
    } else {
        // Menu mode: prefer staged weights; fall back to committed weights from API.
        let variants = effective::effective_variants(feature, ctx.feature_patch.as_ref());
        let (non_control, mut rows, default_suffix) = effective::weight_menu_rows(&variants, |v| {
            v.id.and_then(|id| {
                current_weights
                    .iter()
                    .find(|w| w.variant_id == id)
                    .map(|w| w.weight)
            })
            .unwrap_or(0)
        });

        if non_control.is_empty() {
            bail!("No non-control variants to override. Use `VARIANT add` first.");
        }

        let confirmed = menu::adjust_weights("Adjust variant weights", &mut rows, &default_suffix)?;
        if !confirmed {
            None
        } else {
            let mut parsed = Vec::with_capacity(non_control.len());
            for (v, row) in non_control.iter().zip(rows.iter()) {
                let variant_id = v.id.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Segment overrides require committed variants. Commit the variant first."
                    )
                })?;
                parsed.push(SegmentVariantWeight {
                    variant_id,
                    weight: row.weight,
                });
            }

            // Skip staging if nothing changed.
            if weights_equal(&parsed, &current_weights) {
                None
            } else {
                Some(parsed)
            }
        }
    };

    // Release the read lock before acquiring the write lock below.
    drop(ctx);

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
        anyhow::anyhow!("Not in a feature context. Use \"USE <feature>\" to set a context.")
    })?;
    let feature_id = feature.id;
    let feature_name = feature.name.clone();

    if ctx.segment.is_none() {
        bail!("Not in a segment context. Use \"USE +<segment>\" to set a context.");
    }

    let patch = ctx.get_or_init_segment_patch();

    patch.ops.retain(|op| {
        !matches!(op,
            SegmentPatchOp::SetFeatureOverride { feature_id: fid, .. } |
            SegmentPatchOp::UnsetFeatureOverride { feature_id: fid, .. }
            if *fid == feature_id
        )
    });
    patch
        .ops
        .push(SegmentPatchOp::UnsetFeatureOverride { feature_id });

    println!("Staged: unset segment override for '{feature_name}'");
    Ok(())
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

/// Drop all staged segment changes.
pub fn discard(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.has_segment_pending() {
        ctx.discard_segment_patch();
        println!("Pending changes discarded.");
    }
    Ok(())
}

/// Switch the session into a segment context by name. Clears any active identity
/// context (mutually exclusive with segment context). Shared entry point used by both
/// the top-level `USE +segment` and the `USE feature+segment` shortcut.
pub(crate) fn switch_to(segment_str: &str, session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;

    let segment = fetch_segment(segment_str, session)?;
    let overrides = fetch_overridden_features(segment.id, session);
    segment.display(None, &SegmentContext { overrides });

    let mut ctx = session.context.write().unwrap();
    ctx.variant_index.clear();
    ctx.identity = None;
    ctx.identity_patch = None;
    ctx.segment = Some(segment);

    Ok(())
}
