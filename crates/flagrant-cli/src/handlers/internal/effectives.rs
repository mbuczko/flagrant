//! Helpers for computing the effective (committed + staged) segment/rule, variant and identity/trait state.

use std::collections::{HashMap, HashSet};

use flagrant_types::{
    Comparator, Feature, GroupConnector, IdentityWithTraits, Segment, Subject, TraitValue,
    VariantValue,
    payload::{
        FeaturePatch, IdentityPatch, SegmentPatch, SegmentPatchOp, TagPatchOp, TraitPatchOp,
        VariantPatchOp,
    },
};

use crate::printer::menu;

/// A variant as it appears after applying any staged patch ops.
///
/// Combines committed variants (with `SetValue`/`SetWeight` overrides applied) with
/// staged `Add` variants. Variants with a pending `Delete` op are included but
/// flagged via `is_deleted` so callers can decide whether to show or skip them.
pub(crate) struct EffectiveVariant {
    /// `Some(id)` for committed variants, `None` for staged adds.
    pub id: Option<i32>,
    pub value: VariantValue,
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

/// A tag as it appears after applying any staged patch ops.
///
/// Combines committed tags with staged `Add`s (skipped if already committed). Tags with a
/// pending `Remove` op are included but flagged via `is_deleted`, so callers can decide
/// whether to show or skip them (kept visible by default, so a removal shows red instead
/// of silently vanishing). Sorted by name - unlike variants/traits, tags have no intrinsic
/// order to preserve.
pub(crate) struct EffectiveTag {
    pub name: String,
    /// True for tags that come from a staged `Add` op.
    pub is_staged_add: bool,
    /// True when a staged `Remove` op targets this tag.
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
    pub subject: Subject,
    pub comparator: Comparator,
    pub value: String,
    pub is_staged_add: bool,
    pub is_deleted: bool,
    /// True when a staged `SetRuleValue` op changed the committed value.
    pub value_modified: bool,
    /// True when a staged `SetRuleComparator` op changed the committed comparator.
    pub comparator_modified: bool,
}

pub(crate) struct EffectiveGroup {
    pub label: String,
    pub description: Option<String>,
    pub connector: Option<GroupConnector>,
    pub rules: Vec<EffectiveRule>,
    pub is_staged_add: bool,
    pub is_deleted: bool,
    /// True when a staged `SetGroupDescription` op changed the committed description.
    pub description_modified: bool,
    /// True when a staged `SetGroupConnector` op changed the committed connector.
    pub connector_modified: bool,
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

    let value_overrides: std::collections::HashMap<i32, &VariantValue> = ops
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

/// Builds rows for the "adjust every non-control variant's weight" menu shared by
/// `VARIANT weight` (adjusting the feature's own distribution) and segment `OVERRIDE add`
/// (adjusting a segment's weight override) - the two only differ in where a row's initial
/// weight comes from, supplied via `weight_for`. Returns the same non-control, non-deleted
/// variants zipped 1:1 with the rows (so a caller can map a confirmed row back to its
/// variant), plus the trailing menu row's label for the control/default variant's
/// auto-balanced remainder.
pub(crate) fn weight_menu_rows(
    variants: &[EffectiveVariant],
    weight_for: impl Fn(&EffectiveVariant) -> u8,
) -> (Vec<&EffectiveVariant>, Vec<menu::WeightRow>, String) {
    let non_control: Vec<&EffectiveVariant> = variants
        .iter()
        .filter(|v| !v.is_control && !v.is_deleted)
        .collect();

    let rows = non_control
        .iter()
        .map(|v| {
            let staged = if v.value_modified || v.weight_modified || v.is_staged_add {
                " (staged)"
            } else {
                ""
            };
            menu::WeightRow {
                suffix: format!("{}{staged}", v.value.bare_first_line()),
                weight: weight_for(v),
            }
        })
        .collect();

    let default_suffix = variants
        .iter()
        .find(|v| v.is_control && !v.is_deleted)
        .map(|v| format!("{} (control)", v.value.bare_first_line()))
        .unwrap_or_else(|| "(control)".to_string());

    (non_control, rows, default_suffix)
}

/// Returns the effective tag list for `feature` after applying `patch`.
///
/// Committed tags targeted by a `Remove` op are included but flagged via `is_deleted`.
/// Staged `Add` tags not already committed are appended, flagged via `is_staged_add`. The
/// combined list is sorted by name.
pub(crate) fn effective_tags(feature: &Feature, patch: Option<&FeaturePatch>) -> Vec<EffectiveTag> {
    let ops: &[TagPatchOp] = patch.map(|p| p.tags.as_slice()).unwrap_or_default();
    let names: HashSet<&str> = feature.tags.0.iter().map(|t| t.name.as_str()).collect();

    let mut added: HashSet<&str> = HashSet::new();
    let mut removed: HashSet<&str> = HashSet::new();

    for op in ops {
        match op {
            TagPatchOp::Add(t) if !names.contains(t.as_str()) => {
                added.insert(t.as_str());
            }
            TagPatchOp::Remove(t) if names.contains(t.as_str()) => {
                removed.insert(t.as_str());
            }
            _ => {}
        }
    }

    let mut result: Vec<EffectiveTag> = names
        .iter()
        .map(|&name| EffectiveTag {
            name: name.to_string(),
            is_staged_add: false,
            is_deleted: removed.contains(name),
        })
        .chain(added.iter().map(|&name| EffectiveTag {
            name: name.to_string(),
            is_staged_add: true,
            is_deleted: false,
        }))
        .collect();

    result.sort_by(|a, b| a.name.cmp(&b.name));
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

    let rule_value_overrides: HashMap<i32, &str> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::SetRuleValue { rule_id, value } => Some((*rule_id, value.as_str())),
            _ => None,
        })
        .collect();

    let rule_comparator_overrides: HashMap<i32, &Comparator> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::SetRuleComparator {
                rule_id,
                comparator,
            } => Some((*rule_id, comparator)),
            _ => None,
        })
        .collect();

    let group_desc_overrides: HashMap<&str, Option<&str>> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::SetGroupDescription { label, description } => {
                Some((label.as_str(), description.as_deref()))
            }
            _ => None,
        })
        .collect();

    let group_connector_overrides: HashMap<&str, &GroupConnector> = ops
        .iter()
        .filter_map(|op| match op {
            SegmentPatchOp::SetGroupConnector { label, connector } => {
                Some((label.as_str(), connector))
            }
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
                .map(|r| {
                    let rule_is_deleted = !is_deleted && deleted_rule_ids.contains(&r.id);
                    let value_modified =
                        !rule_is_deleted && rule_value_overrides.contains_key(&r.id);
                    let comparator_modified =
                        !rule_is_deleted && rule_comparator_overrides.contains_key(&r.id);

                    EffectiveRule {
                        subject: r.subject.clone(),
                        comparator: rule_comparator_overrides
                            .get(&r.id)
                            .map(|c| (*c).clone())
                            .unwrap_or_else(|| r.comparator.clone()),
                        value: rule_value_overrides
                            .get(&r.id)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| r.value.clone()),
                        is_staged_add: false,
                        is_deleted: rule_is_deleted,
                        value_modified,
                        comparator_modified,
                    }
                })
                .collect();

            if !is_deleted && let Some(staged) = staged_rules_by_label.get(g.label.as_str()) {
                for op in staged {
                    if let SegmentPatchOp::AddRule {
                        subject,
                        comparator,
                        value,
                        ..
                    } = op
                    {
                        rules.push(EffectiveRule {
                            subject: subject.clone(),
                            comparator: comparator.clone(),
                            value: value.clone(),
                            is_staged_add: true,
                            is_deleted: false,
                            value_modified: false,
                            comparator_modified: false,
                        });
                    }
                }
            }

            let description_modified =
                !is_deleted && group_desc_overrides.contains_key(g.label.as_str());
            let description = if description_modified {
                group_desc_overrides
                    .get(g.label.as_str())
                    .copied()
                    .flatten()
                    .map(|d| d.to_string())
            } else {
                g.description.clone()
            };

            let connector_modified =
                !is_deleted && group_connector_overrides.contains_key(g.label.as_str());
            let connector = if connector_modified {
                group_connector_overrides
                    .get(g.label.as_str())
                    .map(|c| (*c).clone())
            } else {
                g.connector.clone()
            };

            EffectiveGroup {
                label: g.label.clone(),
                description,
                connector,
                rules,
                is_staged_add: false,
                is_deleted,
                description_modified,
                connector_modified,
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
                                subject,
                                comparator,
                                value,
                                ..
                            } = op
                            {
                                Some(EffectiveRule {
                                    subject: subject.clone(),
                                    comparator: comparator.clone(),
                                    value: value.clone(),
                                    is_staged_add: true,
                                    is_deleted: false,
                                    value_modified: false,
                                    comparator_modified: false,
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

            // A later `SetGroupDescription`/`SetGroupConnector` op in the same patch,
            // targeting this group's predicted label, overlays on top of the `AddGroup`
            // op's own initial value - so `GROUP describe`/`GROUP joiner` behave the same
            // whether the group is already committed or still staged.
            let description = match group_desc_overrides.get(label.as_str()) {
                Some(d) => d.map(|s| s.to_string()),
                None => group_desc.clone(),
            };
            let connector = match group_connector_overrides.get(label.as_str()) {
                Some(c) => Some((*c).clone()),
                None => effective_connector,
            };

            groups.push(EffectiveGroup {
                label,
                description,
                connector,
                rules,
                is_staged_add: true,
                is_deleted: false,
                description_modified: false,
                connector_modified: false,
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
