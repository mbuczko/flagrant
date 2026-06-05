//! Helpers for computing the effective (committed + staged) variant and identity state.

use flagrant_types::{
    Feature, FeatureValue, IdentityWithTraits, TraitValue,
    payload::{FeaturePatch, IdentityPatch, TraitPatchOp, VariantPatchOp},
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

/// A trait as it appears after applying any staged patch ops.
///
/// Combines committed traits (with `SetValue` overrides applied) with staged `Add`
/// traits. Traits with a pending `Delete` op are included but flagged via `is_deleted`
/// so callers can decide whether to show or skip them.
pub(crate) struct EffectiveTrait {
    pub name: String,
    pub value: Option<TraitValue>,
    /// True when a staged `SetValue` op changed the committed value.
    pub value_modified: bool,
    /// True for traits that come from a staged `Add` op.
    pub is_staged_add: bool,
    /// True when a staged `Delete` op targets this trait.
    pub is_deleted: bool,
}

/// Returns the effective trait list for `identity` after applying `patch`.
///
/// Committed traits that have a pending `SetValue` op are shown with their new value
/// and `value_modified = true`. Traits targeted by a `Delete` op are included but
/// flagged via `is_deleted`. Staged `Add` traits are appended at the end.
pub(crate) fn effective_identity_traits(
    identity: &IdentityWithTraits,
    patch: Option<&IdentityPatch>,
) -> Vec<EffectiveTrait> {
    let ops: &[TraitPatchOp] = patch.map(|p| p.traits.as_slice()).unwrap_or_default();

    let deleted: std::collections::HashSet<&str> = ops
        .iter()
        .filter_map(|op| match op {
            TraitPatchOp::Delete { name } => Some(name.as_str()),
            _ => None,
        })
        .collect();

    let modified: std::collections::HashMap<&str, &Option<TraitValue>> = ops
        .iter()
        .filter_map(|op| match op {
            TraitPatchOp::SetValue { name, value } => Some((name.as_str(), value)),
            _ => None,
        })
        .collect();

    let mut result: Vec<EffectiveTrait> = identity
        .traits
        .iter()
        .map(|t| {
            let is_deleted = deleted.contains(t.name.as_str());
            let value_modified = !is_deleted && modified.contains_key(t.name.as_str());
            EffectiveTrait {
                name: t.name.clone(),
                value: if value_modified {
                    modified[t.name.as_str()].clone()
                } else {
                    t.value.clone()
                },
                value_modified,
                is_staged_add: false,
                is_deleted,
            }
        })
        .collect();

    for op in ops {
        if let TraitPatchOp::Add { name, value } = op {
            result.push(EffectiveTrait {
                name: name.clone(),
                value: value.clone(),
                value_modified: false,
                is_staged_add: true,
                is_deleted: false,
            });
        }
    }

    result
}
