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

use super::effectives;

/// Resolve a 1-based display index to a [`VariantRef`].
pub(crate) fn resolve(idx: usize, ctx: &Connection) -> anyhow::Result<VariantRef> {
    if ctx.variant_index.is_empty() {
        bail!("Run `FEATURE describe` to refresh indices.")
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

/// Rebuilds the variant index to match the display order produced by `FEATURE describe`.
///
/// Uses `effective_variants` (same sort: descending weight, staged adds last) so that
/// the 1-based numbers the user sees are always in sync with what `resolve` returns.
/// Deleted variants are excluded - they are going away and should not be addressable.
pub(crate) fn rebuild(ctx: &mut Connection) {
    let feature = match ctx.feature.as_ref() {
        Some(f) => f,
        None => {
            ctx.variant_index = vec![];
            return;
        }
    };

    let eff = effectives::effective_variants(feature, ctx.feature_patch.as_ref());
    let mut staged_pos = 0usize;
    let mut index = Vec::with_capacity(eff.len());
    for e in &eff {
        if e.is_deleted {
            continue;
        }
        match e.id {
            Some(id) => index.push(VariantRef::Committed(id)),
            None => {
                index.push(VariantRef::Staged(staged_pos));
                staged_pos += 1;
            }
        }
    }
    ctx.variant_index = index;
}
