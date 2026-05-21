//! Staging helpers for building up a [`FeaturePatch`] or [`IdentityPatch`] before
//! they are committed to the API.

use anyhow::bail;
use flagrant_client::connection::VariantRef;
use flagrant_types::{
    FeatureValue, TraitValue,
    payload::{FeaturePatch, IdentityPatch, TraitPatchOp, VariantPatchOp},
};

/// Upserts a `SetValue` op for a committed variant, or updates the value of a staged `Add` op.
pub(crate) fn stage_value(
    pending: &mut FeaturePatch,
    variant_ref: &VariantRef,
    value: FeatureValue,
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
pub(crate) fn stage_weight(
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
pub(crate) fn discard_feature_patch(pending: &mut FeaturePatch, variant_ref: &VariantRef) {
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

/// Stages a trait value change on an identity patch.
///
/// Uses `SetValue` if the trait already exists on the identity, `Add` otherwise.
/// If a pending op for the same trait name already exists, it is replaced.
pub(crate) fn stage_trait(
    pending: &mut IdentityPatch,
    trait_exists: bool,
    name: String,
    value: TraitValue,
) {
    let op = if trait_exists {
        TraitPatchOp::SetValue { name: name.clone(), value: Some(value.clone()) }
    } else {
        TraitPatchOp::Add { name: name.clone(), value: Some(value.clone()) }
    };
    if let Some(existing) = pending.traits.iter_mut().find(|o| match o {
        TraitPatchOp::Add { name: n, .. }
        | TraitPatchOp::SetValue { name: n, .. }
        | TraitPatchOp::Delete { name: n } => *n == name,
    }) {
        *existing = op;
    } else {
        pending.traits.push(op);
    }
    println!("Staged: {name} = {value}");
}

/// Stages a trait deletion on an identity patch.
///
/// If a pending op for the same trait name already exists, it is replaced.
pub(crate) fn stage_trait_delete(pending: &mut IdentityPatch, name: String) {
    let op = TraitPatchOp::Delete { name: name.clone() };
    if let Some(existing) = pending.traits.iter_mut().find(|o| match o {
        TraitPatchOp::Add { name: n, .. }
        | TraitPatchOp::SetValue { name: n, .. }
        | TraitPatchOp::Delete { name: n } => *n == name,
    }) {
        *existing = op;
    } else {
        pending.traits.push(op);
    }
    println!("Staged: unset {name}");
}

/// Stages an identity rename on an identity patch.
pub(crate) fn stage_identity(pending: &mut IdentityPatch, value: String) {
    pending.identity = Some(value.clone());
    println!("Staged: identity = {value}");
}
