use anyhow::bail;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::command::Arg;
use flagrant_types::{
    Feature,
    payload::{FeaturePatch, VariantPatchOp},
};

/// Resolve a 1-based display index from the last `VARIANT list` output to a VariantRef.
pub(super) fn resolve(raw: &Arg, ctx: &Connection) -> anyhow::Result<VariantRef> {
    let idx: usize = raw.parse::<usize>()?;

    if ctx.variant_index.is_empty() {
        bail!("Run `VARIANT list` to refresh indices.")
    }
    if idx == 0 || idx > ctx.variant_index.len() {
        bail!(
            "Index {} out of range (1–{}).",
            idx,
            ctx.variant_index.len()
        );
    }
    Ok(ctx.variant_index[idx - 1].clone())
}

/// Rebuilds the variant index from the current feature's committed variants and any staged Add ops.
/// Committed variants come first (sorted by id), followed by staged additions in order.
pub(super) fn rebuild(ctx: &mut Connection) {
    let variants = ctx
        .feature
        .as_ref()
        .map(|f| f.variants.as_slice())
        .unwrap_or_default();
    let staged_count = ctx
        .pending
        .as_ref()
        .map(|p| {
            p.variants
                .iter()
                .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                .count()
        })
        .unwrap_or(0);

    let mut sorted_ids: Vec<i32> = variants.iter().map(|v| v.id).collect();
    sorted_ids.sort_unstable();

    let mut index: Vec<VariantRef> = sorted_ids.into_iter().map(VariantRef::Committed).collect();
    for staged_pos in 0..staged_count {
        index.push(VariantRef::Staged(staged_pos));
    }
    ctx.variant_index = index;
}

/// Returns the current bare value (without type prefix) for a variant, used to pre-fill the
/// editor. For committed variants, prefers any already-staged `SetValue` op; for staged
/// (Add) variants, returns the value from the pending `Add` op.
pub(super) fn current_variant_value(variant_ref: &VariantRef, ctx: &Connection) -> String {
    match variant_ref {
        VariantRef::Committed(id) => {
            let staged = ctx.pending.as_ref().and_then(|p| {
                p.variants.iter().find_map(|op| match op {
                    VariantPatchOp::SetValue { id: oid, value } if oid == id => Some(value.clone()),
                    _ => None,
                })
            });
            staged.unwrap_or_else(|| {
                ctx.feature
                    .as_ref()
                    .and_then(|f| f.variants.iter().find(|v| v.id == *id))
                    .map(|v| v.value.decompose().1.to_owned())
                    .unwrap_or_default()
            })
        }
        VariantRef::Staged(staged_pos) => ctx
            .pending
            .as_ref()
            .and_then(|p| {
                p.variants
                    .iter()
                    .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                    .nth(*staged_pos)
                    .and_then(|op| match op {
                        VariantPatchOp::Add { value, .. } => Some(value.clone()),
                        _ => None,
                    })
            })
            .unwrap_or_default(),
    }
}

/// Upserts a `SetValue` op for a committed variant, or updates the value of a staged `Add` op.
pub(super) fn stage_value(
    pending: &mut FeaturePatch,
    variant_ref: &VariantRef,
    value: String,
) -> anyhow::Result<()> {
    let ops = &mut pending.variants;
    match variant_ref {
        VariantRef::Committed(id) => {
            if let Some(op) = ops
                .iter_mut()
                .find(|op| matches!(op, VariantPatchOp::SetValue { id: oid, .. } if oid == id))
            {
                *op = VariantPatchOp::SetValue {
                    id: *id,
                    value: value.clone(),
                };
            } else {
                ops.push(VariantPatchOp::SetValue {
                    id: *id,
                    value: value.clone(),
                });
            }
            println!("Staged: variant value id={id} value={value}");
        }
        VariantRef::Staged(staged_pos) => {
            let add_op = ops
                .iter_mut()
                .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                .nth(*staged_pos);
            match add_op {
                Some(VariantPatchOp::Add { value: v, .. }) => {
                    *v = value.clone();
                    println!("Updated staged variant value to {value}");
                }
                _ => bail!("Staged variant not found."),
            }
        }
    }
    Ok(())
}

/// Upserts a `SetWeight` op for a committed variant, or updates the weight of a staged `Add` op.
pub(super) fn stage_weight(
    pending: &mut FeaturePatch,
    variant_ref: &VariantRef,
    weight: u8,
) -> anyhow::Result<()> {
    let ops = &mut pending.variants;
    match variant_ref {
        VariantRef::Committed(id) => {
            if let Some(op) = ops
                .iter_mut()
                .find(|op| matches!(op, VariantPatchOp::SetWeight { id: oid, .. } if oid == id))
            {
                *op = VariantPatchOp::SetWeight { id: *id, weight };
            } else {
                ops.push(VariantPatchOp::SetWeight { id: *id, weight });
            }
            println!("Staged: variant weight id={id} weight={weight}");
        }
        VariantRef::Staged(staged_pos) => {
            let add_op = ops
                .iter_mut()
                .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                .nth(*staged_pos);
            match add_op {
                Some(VariantPatchOp::Add { weight: w, .. }) => {
                    *w = weight;
                    println!("Updated staged variant weight to {weight}");
                }
                _ => bail!("Staged variant not found."),
            }
        }
    }
    Ok(())
}

/// Discards all pending ops for the given variant ref from the patch.
/// For committed variants, removes any SetValue / SetWeight / Delete ops by id.
/// For staged variants, removes the corresponding Add op by its position.
pub(super) fn discard_pending(pending: &mut FeaturePatch, variant_ref: &VariantRef) {
    match variant_ref {
        VariantRef::Committed(id) => {
            let before = pending.variants.len();
            pending.variants.retain(|op| {
                !matches!(op,
                    VariantPatchOp::SetValue { id: oid, .. }
                    | VariantPatchOp::SetWeight { id: oid, .. }
                    | VariantPatchOp::Delete { id: oid }
                    if oid == id
                )
            });
            if pending.variants.len() == before {
                println!("No pending changes for variant id={id}.");
            } else {
                println!("Discarded pending changes for variant id={id}.");
            }
        }
        VariantRef::Staged(staged_pos) => {
            let mut add_count = 0;
            let mut remove_at = None;
            for (i, op) in pending.variants.iter().enumerate() {
                if matches!(op, VariantPatchOp::Add { .. }) {
                    if add_count == *staged_pos {
                        remove_at = Some(i);
                        break;
                    }
                    add_count += 1;
                }
            }
            match remove_at {
                Some(i) => {
                    pending.variants.remove(i);
                    println!("Discarded staged variant addition.");
                }
                None => println!("Staged variant not found."),
            }
        }
    }
}

/// Computes the total weight of all non-control variants, applying pending overrides and
/// substituting `new_weight` for the variant identified by `variant_ref`.
pub(super) fn total_non_control_weight(
    feature: &Feature,
    pending: Option<&FeaturePatch>,
    variant_ref: &VariantRef,
    new_weight: u8,
) -> u32 {
    let deleted_ids: std::collections::HashSet<i32> = pending
        .map(|p| {
            p.variants
                .iter()
                .filter_map(|op| match op {
                    VariantPatchOp::Delete { id } => Some(*id),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let committed: u32 = feature
        .variants
        .iter()
        .filter(|v| !v.is_control() && !deleted_ids.contains(&v.id))
        .map(|v| {
            (match variant_ref {
                VariantRef::Committed(id) if *id == v.id => new_weight,
                _ => pending
                    .and_then(|p| {
                        p.variants.iter().find_map(|op| match op {
                            VariantPatchOp::SetWeight { id, weight } if *id == v.id => {
                                Some(*weight)
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or(v.weight),
            }) as u32
        })
        .sum();

    let staged: u32 = pending
        .map(|p| {
            p.variants
                .iter()
                .enumerate()
                .filter(|(_, op)| matches!(op, VariantPatchOp::Add { .. }))
                .map(|(i, op)| match op {
                    VariantPatchOp::Add { weight, .. } => match variant_ref {
                        VariantRef::Staged(pos) if *pos == i => new_weight as u32,
                        _ => *weight as u32,
                    },
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0);

    committed + staged
}
