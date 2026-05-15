//! Variant index management.
//!
//! This module maintains the positional index that maps 1-based display numbers
//! (as shown by `VARIANT list`) to [`VariantRef`] values.
//!
//! The index is rebuilt after every mutation via [`rebuild`]. Committed variants
//! always appear first (sorted by id), followed by any staged additions in
//! insertion order. [`resolve`] translates a user-supplied 1-based number back
//! to a [`VariantRef`] so the caller can identify which variant to act on.

use anyhow::bail;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_types::payload::VariantPatchOp;

/// Resolve a 1-based display index to a [`VariantRef`].
pub(crate) fn resolve(idx: usize, ctx: &Connection) -> anyhow::Result<VariantRef> {
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
pub(crate) fn rebuild(ctx: &mut Connection) {
    let variants = ctx
        .feature
        .as_ref()
        .map(|f| f.variants.as_slice())
        .unwrap_or_default();
    let staged_count = ctx
        .feature_patch
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

