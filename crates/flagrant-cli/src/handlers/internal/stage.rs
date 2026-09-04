//! Staging helpers for building up a [`FeaturePatch`], [`IdentityPatch`], or
//! [`SegmentPatch`] before they are committed to the API.

use std::borrow::Cow;

use anyhow::bail;
use flagrant_client::connection::{Connection, RuleRef, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Comparator, Environment, TraitValue, VariantValue,
    payload::{
        CommitPayload, CommitResult, FeatureCommitPart, FeaturePatch, IdentityCommitPart,
        IdentityPatch, SegmentCommitPart, SegmentPatch, SegmentPatchOp, TagPatchOp, TraitPatchOp,
        VariantPatchOp,
    },
};

use super::index;
use crate::handlers::{features, identities, segments};

/// Bail if any context has uncommitted staged changes.
///
/// Call this at the top of every context-switching handler (`use`, `add`) before
/// clearing and replacing the current context.
pub(crate) fn ensure_no_pending(session: &Session<Connection>) -> anyhow::Result<()> {
    if session.context.read().unwrap().has_any_pending() {
        bail!("You have uncommitted changes. Run `COMMIT` or `DISCARD` first.");
    }
    Ok(())
}

/// Stages a `SetValue` op for a committed variant, or updates the value of a staged `Add` op.
pub(crate) fn stage_value(
    pending: &mut FeaturePatch,
    variant_ref: &VariantRef,
    value: VariantValue,
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

/// Stages a `SetWeight` op for a committed variant, or updates the weight of a staged `Add` op.
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

/// Stages a `SetRuleValue` op for a committed rule, or updates the value of a staged
/// `AddRule` op in place.
pub(crate) fn stage_rule_value(patch: &mut SegmentPatch, target: &RuleRef, value: String) {
    match target {
        RuleRef::Committed(rule_id) => {
            let op = SegmentPatchOp::SetRuleValue {
                rule_id: *rule_id,
                value,
            };
            if let Some(existing) = patch.ops.iter_mut().find(
                |o| matches!(o, SegmentPatchOp::SetRuleValue { rule_id: rid, .. } if rid == rule_id),
            ) {
                *existing = op;
            } else {
                patch.ops.push(op);
            }
        }
        RuleRef::Staged {
            group_label,
            position,
        } => {
            if let Some(op) = staged_add_rule_op_mut(patch, group_label, *position)
                && let SegmentPatchOp::AddRule { value: v, .. } = op
            {
                *v = value;
            }
        }
    }
}

/// Stages a `SetRuleComparator` op for a committed rule, or updates the comparator of a
/// staged `AddRule` op in place.
pub(crate) fn stage_rule_comparator(
    patch: &mut SegmentPatch,
    target: &RuleRef,
    comparator: Comparator,
) {
    match target {
        RuleRef::Committed(rule_id) => {
            let op = SegmentPatchOp::SetRuleComparator {
                rule_id: *rule_id,
                comparator,
            };
            if let Some(existing) = patch.ops.iter_mut().find(
                |o| matches!(o, SegmentPatchOp::SetRuleComparator { rule_id: rid, .. } if rid == rule_id),
            ) {
                *existing = op;
            } else {
                patch.ops.push(op);
            }
        }
        RuleRef::Staged {
            group_label,
            position,
        } => {
            if let Some(op) = staged_add_rule_op_mut(patch, group_label, *position)
                && let SegmentPatchOp::AddRule { comparator: c, .. } = op
            {
                *c = comparator;
            }
        }
    }
}

/// Discards a rule: stages a `DeleteRule` op for a committed rule, or discards the pending
/// `AddRule` op outright for a staged addition.
pub(crate) fn discard_rule(patch: &mut SegmentPatch, target: &RuleRef) {
    match target {
        RuleRef::Committed(rule_id) => {
            patch
                .ops
                .push(SegmentPatchOp::DeleteRule { rule_id: *rule_id });
        }
        RuleRef::Staged {
            group_label,
            position,
        } => {
            if let Some(i) = staged_add_rule_op_index(patch, group_label, *position) {
                patch.ops.remove(i);
            }
        }
    }
}

/// Finds the index into `patch.ops` of the `AddRule` op at `position` (0-based, among this
/// group's staged rules, in `ops` order) for `group_label`. `effective_segment` appends
/// staged rules to a group's effective rule list in the same order their `AddRule` ops
/// appear in `ops`, so `position` (computed by `rules::resolve_rule`) and the op found here
/// always agree.
fn staged_add_rule_op_index(
    patch: &SegmentPatch,
    group_label: &str,
    position: usize,
) -> Option<usize> {
    let mut add_count = 0;
    patch.ops.iter().position(|op| {
        matches!(op, SegmentPatchOp::AddRule { group_label: gl, .. } if gl == group_label) && {
            let is_target = add_count == position;
            add_count += 1;
            is_target
        }
    })
}

/// Same lookup as [`staged_add_rule_op_index`], but returns a mutable reference to the op
/// itself for in-place edits (used by `stage_rule_value`/`stage_rule_comparator`).
fn staged_add_rule_op_mut<'a>(
    patch: &'a mut SegmentPatch,
    group_label: &str,
    position: usize,
) -> Option<&'a mut SegmentPatchOp> {
    let mut add_count = 0;
    patch.ops.iter_mut().find(|op| {
        matches!(op, SegmentPatchOp::AddRule { group_label: gl, .. } if gl == group_label) && {
            let is_target = add_count == position;
            add_count += 1;
            is_target
        }
    })
}

/// Stages a tag addition or removal on a feature patch.
///
/// If a pending op for the same tag already exists, it is replaced - so staging
/// `Remove` after a pending `Add` for the same tag cancels the addition out, and
/// vice versa.
pub(crate) fn stage_tag(pending: &mut FeaturePatch, tag: String, add: bool) {
    let existing = pending.tags.iter_mut().find(|o| match o {
        TagPatchOp::Add(t) | TagPatchOp::Remove(t) => *t == tag,
    });
    let op = if add {
        TagPatchOp::Add(tag)
    } else {
        TagPatchOp::Remove(tag)
    };
    match existing {
        Some(slot) => *slot = op,
        None => pending.tags.push(op),
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
        TraitPatchOp::SetValue {
            name: name.clone(),
            value: Some(value.clone()),
        }
    } else {
        TraitPatchOp::Add {
            name: name.clone(),
            value: Some(value.clone()),
        }
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

/// Commits all staged changes across active contexts (feature, identity, and/or segment)
/// as a single atomic server-side operation - see [`CommitPayload`] / `commit::apply` on
/// the API side. A trailing argument, if given, becomes the comment recorded on every
/// snapshot this commit produces.
///
/// Whichever of `ctx.feature_patch` / `ctx.identity_patch` / `ctx.segment_patch` are
/// pending are sent together in one request and applied together in one transaction - this
/// used to be up to three independent `PATCH` calls (one per context), which meant a
/// partial failure across them wasn't rolled back, and (once segment/identity overrides
/// could affect a feature's snapshot history) risked recording multiple uncoordinated
/// snapshots from what the user experienced as a single `COMMIT`.
pub(crate) fn commit(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let has_feature = ctx.has_feature_pending() && ctx.feature.is_some();
    let has_identity = ctx.identity.is_some() && ctx.has_identity_pending();
    let has_segment = ctx.segment.is_some() && ctx.has_segment_pending();

    if !has_feature && !has_identity && !has_segment {
        println!("No pending changes to commit.");
        return Ok(());
    }

    let comment = args.first().map(|a| a.to_string());
    let current_env_id = ctx.environment.id;
    let current_env_name = ctx.environment.name.clone();
    let feature_id = ctx.feature.as_ref().map(|f| f.id);
    let identity_value = ctx.identity.as_ref().map(|i| i.value.clone());

    let payload = CommitPayload {
        comment,
        feature: has_feature.then(|| FeatureCommitPart {
            id: feature_id.unwrap(),
            version: ctx.feature.as_ref().map(|f| f.version),
            patch: ctx.feature_patch.clone().unwrap(),
        }),
        identity: has_identity.then(|| IdentityCommitPart {
            value: identity_value.clone().unwrap(),
            patch: ctx.identity_patch.clone().unwrap(),
        }),
        segment: has_segment.then(|| SegmentCommitPart {
            id: ctx.segment.as_ref().unwrap().id,
            version: ctx.segment.as_ref().map(|s| s.version),
            patch: ctx.segment_patch.clone().unwrap(),
        }),
    };

    let path = ctx.env_resource().subpath("/commit");
    let result: CommitResult = ctx
        .client
        .post(path, payload)
        .map_err(|err| anyhow::anyhow!("Commit failed: {err}"))?;

    if has_feature {
        ctx.feature_patch = None;
        match result.feature {
            Some(updated) => {
                ctx.feature = Some(updated);
                index::rebuild(&mut ctx);
            }
            None => {
                println!("Feature '{}' deleted.", ctx.feature.as_ref().unwrap().name);
                ctx.feature = None;
                ctx.variant_index.clear();
            }
        }
    }

    if has_identity {
        ctx.identity_patch = None;
        match result.identity {
            Some(updated) => ctx.identity = Some(updated),
            None => {
                println!("Identity '{}' deleted.", identity_value.unwrap());
                ctx.identity = None;
            }
        }
    }

    if has_segment {
        ctx.segment_patch = None;
        match result.segment {
            Some(updated) => ctx.segment = Some(updated),
            None => {
                println!("Segment '{}' deleted.", ctx.segment.as_ref().unwrap().name);
                ctx.segment = None;
            }
        }
    }
    drop(ctx);

    for snapshot in &result.snapshots {
        // The feature name is captured in the snapshot's own state, so no extra lookup is
        // needed for it - falls back to the id only if the state somehow fails to parse.
        let feature_name = snapshot
            .parsed_state()
            .map(|s| s.name)
            .unwrap_or_else(|_| format!("#{}", snapshot.feature_id));

        let env_name = if snapshot.environment_id == current_env_id {
            Cow::Borrowed(current_env_name.as_str())
        } else {
            Cow::Owned(environment_name(session, snapshot.environment_id))
        };
        println!(
            "Snapshot v{} recorded for '{feature_name}' (environment '{env_name}').",
            snapshot.version
        );
    }

    Ok(())
}

/// Resolves an environment id to its display name via the API, falling back to `#{id}` if
/// the lookup fails - only needed for the rare cross-environment snapshot notice (a
/// structural segment change cascades to every environment of the project, not just the
/// one in context - see `commit::apply`), where the name isn't otherwise available.
fn environment_name(session: &Session<Connection>, environment_id: i32) -> String {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();

    ctx.client
        .get::<Environment>(res.subpath(format!("/envs/{environment_id}")))
        .map(|e| e.name)
        .unwrap_or_else(|_| format!("#{environment_id}"))
}

/// Resets feature, identity, and segment contexts, clearing all state.
///
/// Refuses to run if there are any uncommitted staged changes - run `COMMIT` or
/// `DISCARD` first to avoid losing work.
pub(crate) fn reset(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    {
        let ctx = session.context.read().unwrap();
        let has_pending_feature = ctx
            .feature_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false);
        if has_pending_feature || ctx.has_identity_pending() || ctx.has_segment_pending() {
            anyhow::bail!("You have uncommitted changes. Run `COMMIT` or `DISCARD` first.");
        }
    }

    let mut ctx = session.context.write().unwrap();

    ctx.feature = None;
    ctx.feature_patch = None;
    ctx.identity = None;
    ctx.identity_patch = None;
    ctx.segment = None;
    ctx.segment_patch = None;
    ctx.variant_index.clear();

    println!("Context reset.");
    Ok(())
}

/// Discards all staged changes across active contexts (feature, identity, and/or segment).
pub(crate) fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let has_feature = ctx.feature.is_some()
        && ctx
            .feature_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false);
    let has_identity = ctx.identity.is_some() && ctx.has_identity_pending();
    let has_segment = ctx.segment.is_some() && ctx.has_segment_pending();
    drop(ctx);

    if !has_feature && !has_identity && !has_segment {
        println!("No pending changes.");
        return Ok(());
    }

    if has_feature {
        features::discard(args, session)?;
    }
    if has_identity {
        identities::discard(args, session)?;
    }
    if has_segment {
        segments::discard(args, session)?;
    }
    Ok(())
}
