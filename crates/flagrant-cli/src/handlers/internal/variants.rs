//! Helpers for computing the effective (committed + staged) variant state,
//! and for fetching all variant assignments for an identity across features.

use flagrant_client::connection::Connection;
use flagrant_types::{
    Feature, FeatureValue, IdentityVariant,
    payload::{FeaturePatch, VariantPatchOp},
};

/// A variant as it appears after applying any staged patch ops.
///
/// Combines committed variants (with `SetValue`/`SetWeight` overrides applied) with
/// staged `Add` variants. Variants with a pending `Delete` op are included but
/// flagged via `is_deleted` so callers can decide whether to show or skip them.
pub(crate) struct EffectiveVariant {
    /// `Some(id)` for committed variants, `None` for staged adds.
    pub id: Option<i32>,
    pub value: FeatureValue,
    pub weight: u8,
    pub is_control: bool,
    /// True when a staged `SetValue` op changed the committed value.
    pub value_modified: bool,
    /// True when a staged `SetWeight` op changed the committed weight.
    pub weight_modified: bool,
    /// True for variants that come from a staged `Add` op.
    pub is_staged_add: bool,
    /// True when a staged `Delete` op targets this variant.
    pub is_deleted: bool,
}

/// Returns the effective variant list for `feature` after applying `patch`.
///
/// Committed variants that have a pending `Delete` op are omitted. For the
/// remaining committed variants, any `SetValue`/`SetWeight` ops are applied.
/// Staged `Add` variants are appended at the end, after all committed ones.
/// The control variant (if present and not deleted) is always last.
pub(crate) fn effective_variants(
    feature: &Feature,
    patch: Option<&FeaturePatch>,
) -> Vec<EffectiveVariant> {
    let ops: &[VariantPatchOp] = patch.map(|p| p.variants.as_slice()).unwrap_or_default();

    let deleted_ids: std::collections::HashSet<i32> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::Delete { id } => Some(*id),
            _ => None,
        })
        .collect();

    let value_overrides: std::collections::HashMap<i32, &FeatureValue> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::SetValue { id, value } => Some((*id, value)),
            _ => None,
        })
        .collect();

    let weight_overrides: std::collections::HashMap<i32, u8> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::SetWeight { id, weight } => Some((*id, *weight)),
            _ => None,
        })
        .collect();

    let mut result: Vec<EffectiveVariant> = feature
        .variants
        .iter()
        .map(|v| {
            let is_deleted = deleted_ids.contains(&v.id);
            let value_modified = !is_deleted && value_overrides.contains_key(&v.id);
            let weight_modified = !is_deleted && weight_overrides.contains_key(&v.id);
            EffectiveVariant {
                id: Some(v.id),
                value: value_overrides
                    .get(&v.id)
                    .copied()
                    .cloned()
                    .unwrap_or_else(|| v.value.clone()),
                weight: weight_overrides.get(&v.id).copied().unwrap_or(v.weight),
                is_control: v.is_control(),
                value_modified,
                weight_modified,
                is_staged_add: false,
                is_deleted,
            }
        })
        .collect();

    // Sort committed variants by descending effective weight; staged adds are appended last.
    result.sort_by_key(|e| std::cmp::Reverse(e.weight));

    for op in ops {
        if let VariantPatchOp::Add { value, weight } = op {
            result.push(EffectiveVariant {
                id: None,
                value: value.clone(),
                weight: *weight,
                is_control: false,
                value_modified: false,
                weight_modified: false,
                is_staged_add: true,
                is_deleted: false,
            });
        }
    }

    result
}


/// Fetches all variant assignments for `identity_value` across every feature in the active environment.
///
/// Used when no feature context is active to give a full picture of where the identity is assigned.
/// Returns an empty vec if the identity has no assignments or the request fails.
pub(crate) fn fetch_all_variant_assignments(
    ctx: &Connection,
    identity_value: &str,
) -> Vec<IdentityVariant> {
    let path = ctx
        .env_resource()
        .subpath(format!("/identities/{identity_value}/variants"));
    ctx.client
        .get::<Vec<IdentityVariant>>(path)
        .unwrap_or_default()
}
