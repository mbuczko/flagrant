//! Helpers for computing the effective (committed + staged) variant and identity state.

use std::collections::{HashMap, HashSet};

use flagrant_types::{
    Comparator, Feature, FeatureValue, GroupConnector, IdentityWithTraits, Segment, SegmentDriver,
    TraitValue,
    payload::{
        FeaturePatch, IdentityPatch, SegmentPatch, SegmentPatchOp, TraitPatchOp, VariantPatchOp,
    },
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

pub(crate) struct EffectiveRule {
    pub driver: SegmentDriver,
    pub comparator: Comparator,
    pub value: String,
    pub is_staged_add: bool,
    pub is_deleted: bool,
}

pub(crate) struct EffectiveGroup {
    pub label: String,
    pub description: Option<String>,
    pub connector: Option<GroupConnector>,
    pub rules: Vec<EffectiveRule>,
    pub is_staged_add: bool,
    pub is_deleted: bool,
}

pub(crate) struct EffectiveSegment {
    pub name: String,
    pub description: Option<String>,
    pub name_modified: bool,
    pub description_modified: bool,
    pub groups: Vec<EffectiveGroup>,
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

/// Returns the effective segment state after applying `patch` on top of the committed `segment`.
///
/// Committed groups/rules with pending deletion ops are flagged via `is_deleted`.
/// Staged group/rule additions are appended and flagged via `is_staged_add`.
/// `SetName`/`SetDescription` ops are reflected in `name_modified`/`description_modified`.
pub(crate) fn effective_segment(
    segment: &Segment,
    patch: Option<&SegmentPatch>,
) -> EffectiveSegment {
    let ops = patch.map(|p| p.ops.as_slice()).unwrap_or_default();

    let mut name = segment.name.clone();
    let mut description = segment.description.clone();
    let mut name_modified = false;
    let mut description_modified = false;

    for op in ops {
        match op {
            SegmentPatchOp::SetName(n) => {
                name = n.clone();
                name_modified = true;
            }
            SegmentPatchOp::SetDescription(d) => {
                description = d.clone();
                description_modified = true;
            }
            _ => {}
        }
    }

    let deleted_labels: HashSet<&str> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::DeleteGroup { label } => Some(label.as_str()),
            _ => None,
        })
        .collect();

    let deleted_rule_ids: HashSet<i32> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::DeleteRule { rule_id } => Some(*rule_id),
            _ => None,
        })
        .collect();

    let mut staged_rules_by_label: HashMap<&str, Vec<&SegmentPatchOp>> = HashMap::new();
    for op in ops {
        if let SegmentPatchOp::AddRule { group_label, .. } = op {
            staged_rules_by_label
                .entry(group_label.as_str())
                .or_default()
                .push(op);
        }
    }

    let mut groups: Vec<EffectiveGroup> = segment
        .groups
        .iter()
        .map(|g| {
            let is_deleted = deleted_labels.contains(g.label.as_str());
            let mut rules: Vec<EffectiveRule> = g
                .rules
                .iter()
                .map(|r| EffectiveRule {
                    driver: r.driver.clone(),
                    comparator: r.comparator.clone(),
                    value: r.value.clone(),
                    is_staged_add: false,
                    is_deleted: !is_deleted && deleted_rule_ids.contains(&r.id),
                })
                .collect();

            if !is_deleted && let Some(staged) = staged_rules_by_label.get(g.label.as_str()) {
                for op in staged {
                    if let SegmentPatchOp::AddRule {
                        driver,
                        comparator,
                        value,
                        ..
                    } = op
                    {
                        rules.push(EffectiveRule {
                            driver: driver.clone(),
                            comparator: comparator.clone(),
                            value: value.clone(),
                            is_staged_add: true,
                            is_deleted: false,
                        });
                    }
                }
            }

            EffectiveGroup {
                label: g.label.clone(),
                description: g.description.clone(),
                connector: g.connector.clone(),
                rules,
                is_staged_add: false,
                is_deleted,
            }
        })
        .collect();

    // Append staged AddGroup ops with predicted labels.
    let mut max_n: u32 = segment
        .groups
        .iter()
        .filter_map(|g| g.label.strip_prefix("group-"))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    let mut effective_count = segment
        .groups
        .iter()
        .filter(|g| !deleted_labels.contains(g.label.as_str()))
        .count();

    for op in ops {
        if let SegmentPatchOp::AddGroup {
            connector,
            description: group_desc,
        } = op
        {
            max_n += 1;
            let label = format!("group-{max_n}");

            let rules = staged_rules_by_label
                .get(label.as_str())
                .map(|staged| {
                    staged
                        .iter()
                        .filter_map(|op| {
                            if let SegmentPatchOp::AddRule {
                                driver,
                                comparator,
                                value,
                                ..
                            } = op
                            {
                                Some(EffectiveRule {
                                    driver: driver.clone(),
                                    comparator: comparator.clone(),
                                    value: value.clone(),
                                    is_staged_add: true,
                                    is_deleted: false,
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let effective_connector = if effective_count == 0 {
                None
            } else {
                connector.clone().or(Some(GroupConnector::And))
            };
            effective_count += 1;

            groups.push(EffectiveGroup {
                label,
                description: group_desc.clone(),
                connector: effective_connector,
                rules,
                is_staged_add: true,
                is_deleted: false,
            });
        }
    }

    EffectiveSegment {
        name,
        description,
        name_modified,
        description_modified,
        groups,
    }
}
