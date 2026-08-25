use anyhow::bail;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, Variant, VariantValue,
    payload::{FeaturePatch, VariantPatchOp},
};

use crate::{
    handlers::{
        internal::{effectives as effective, index, stage},
        open_in_editor,
    },
    printer::{menu, tabular::Tabular},
};

/// Stage a new variant addition with a given weight and value.
///
/// Expects args: `<weight> [value]`
///
/// If value is omitted, opens `$EDITOR` for interactive input. Fails if the
/// new weight would push total non-control weight over 100%, or if the value is typed
/// (explicitly via `json::...`, or auto-detected from a leading `{`) as `json` but doesn't
/// actually parse as valid JSON.
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let weight = match args.get(1) {
        Some(w) => w.parse::<u8>()?,
        None => bail!("No weight provided."),
    };
    let value = match args.get(2) {
        Some(v) => v.to_string(),
        None => open_in_editor("")?,
    };

    if !(0..=100).contains(&weight) {
        bail!("Variant weight should be positive number in range of <0, 100>.")
    }

    let total = weight as u32
        + total_non_control_weight(
            ctx.feature.as_ref().unwrap(),
            ctx.feature_patch.as_ref(),
            &VariantRef::Staged(usize::MAX),
            weight,
        );

    if total > 100 {
        bail!("Total weight of non-control variants would be {total}%, exceeding 100%.");
    }

    let vv: VariantValue = value
        .parse()
        .unwrap_or_else(|_| VariantValue::build(&value));

    if let VariantValue::Json(json) = &vv {
        serde_json::from_str::<serde_json::Value>(json)
            .map_err(|e| anyhow::anyhow!("Value is not valid JSON: {e}"))?;
    }

    let feature = ctx.feature.as_ref().unwrap();
    if feature.variants.iter().any(|v| v.value == vv)
        || ctx.feature_patch.as_ref().is_some_and(|p| {
            p.variants
                .iter()
                .any(|op| matches!(op, VariantPatchOp::Add { value, .. } if *value == vv))
        })
    {
        bail!("A variant with this value already exists for this feature.");
    }

    println!("Staged: variant add weight={weight} value={value}");

    ctx.get_or_init_feature_patch()
        .variants
        .push(VariantPatchOp::Add { value: vv, weight });

    index::rebuild(&mut ctx);
    Ok(())
}

/// Print details of a single variant identified by its display index: weight, full
/// (possibly multi-line) value, whether it's the control variant, and any identities
/// explicitly pinned to it.
///
/// Expected args: `<index>`
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let index: usize = match args.get(1) {
        Some(idx) => idx.parse()?,
        None => bail!("No variant index provided."),
    };

    // Validates the index against the current variant index (built the same way by
    // `index::rebuild`), mirroring the range check used by value/weight/delete.
    index::resolve(index, &ctx)?;

    let feature = ctx.feature.as_ref().unwrap();
    let eff = effective::effective_variants(feature, ctx.feature_patch.as_ref());
    let variant = eff
        .iter()
        .filter(|e| !e.is_deleted)
        .nth(index - 1)
        .ok_or_else(|| anyhow::anyhow!("No variant at index {index}."))?;

    let identities = match variant.id {
        Some(id) => fetch_pinned_identities(id, session),
        None => Vec::new(),
    };

    variant.display(None, &(index, identities));
    Ok(())
}

/// Fetches identity values explicitly pinned to the given committed variant.
fn fetch_pinned_identities(variant_id: i32, session: &Session<Connection>) -> Vec<String> {
    let ctx = session.context.read().unwrap();
    let res = ctx.env_resource();

    ctx.client
        .get::<Vec<String>>(res.subpath(format!("/variants/{variant_id}/identities")))
        .unwrap_or_default()
}

/// Stage a value change for an existing variant identified by its display index.
///
/// Expected args: `[value]`
///
/// If the value argument is omitted, opens `$EDITOR` pre-filled with the current
/// value so the user can edit it interactively.
pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => index::resolve(idx.parse::<usize>()?, &ctx)?,
        None => bail!("No variant index provided."),
    };
    let raw = match args.get(2) {
        Some(v) => v.to_string(),
        None => open_in_editor(current_variant_value(&variant_ref, &ctx).decompose().1)?,
    };
    let current = current_variant_value(&variant_ref, &ctx);
    let vv = raw
        .parse::<VariantValue>()
        .unwrap_or_else(|_| current.clone_with(raw.trim()));

    if vv == current {
        return Ok(());
    }

    let feature = ctx.feature.as_ref().unwrap();
    let duplicate = feature.variants.iter().any(|v| {
        v.value == vv && !matches!(&variant_ref, VariantRef::Committed(id) if *id == v.id)
    }) || ctx.feature_patch.as_ref().is_some_and(|p| {
        p.variants.iter().enumerate().any(|(i, op)| match op {
            VariantPatchOp::Add { value, .. } => {
                *value == vv && !matches!(&variant_ref, VariantRef::Staged(pos) if *pos == i)
            }
            VariantPatchOp::SetValue { id, value } => {
                *value == vv && !matches!(&variant_ref, VariantRef::Committed(vid) if *vid == *id)
            }
            _ => false,
        })
    });

    if duplicate {
        bail!("A variant with this value already exists for this feature.");
    }

    // Keep any staged identity override in sync: if it was pointing at the old value,
    // update it to the new value so it doesn't become stale after commit.
    let feature_name = ctx.feature.as_ref().unwrap().name.clone();

    if let Some(patch) = ctx.identity_patch.as_mut() {
        for o in patch.overrides.iter_mut() {
            if o.feature_name == feature_name && o.variant_value == current {
                o.variant_value = vv.clone();
                println!("Updated staged override value: '{current}' → '{vv}'");
            }
        }
    }

    stage::stage_value(ctx.get_or_init_feature_patch(), &variant_ref, vv)?;
    index::rebuild(&mut ctx);
    Ok(())
}

/// Stage a weight change for an existing variant identified by its display index.
///
/// Expected args: `[index [+/-]weight]`
///
/// Weight may be an absolute value (e.g. `30`) or a relative change prefixed with `+` or `-`
/// (e.g. `+5` adds 5 to the current weight, `-3` subtracts 3). Refuses to change the control
/// variant's weight (it is auto-adjusted) and rejects values that would push total non-control
/// weight over 100%.
///
/// When called with no arguments at all, opens an interactive menu listing every
/// non-control variant's weight instead.
pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }

    let Some(idx_arg) = args.get(1) else {
        return adjust_weights_interactively(&mut ctx);
    };

    let variant_ref = index::resolve(idx_arg.parse::<usize>()?, &ctx)?;
    let new_weight: u8 = match args.get(2) {
        Some(w) if w.starts_with('+') || w.starts_with('-') => {
            let delta = w.parse::<i16>()?;
            let current = current_variant_weight(&variant_ref, &ctx);
            (current + delta)
                .try_into()
                .map_err(|_| anyhow::anyhow!("Resulting weight out of range <0, 100>."))?
        }
        Some(w) => w.parse::<u8>()?,
        None => bail!("No weight provided."),
    };
    if !(0..=100).contains(&new_weight) {
        bail!("Variant weight should be positive number in range of <0, 100>.")
    }
    if new_weight == current_variant_weight(&variant_ref, &ctx) as u8 {
        return Ok(());
    }
    if let VariantRef::Committed(id) = &variant_ref {
        let is_control = ctx
            .feature
            .as_ref()
            .and_then(|f| f.variants.iter().find(|v| v.id == *id))
            .map(|v| v.is_control())
            .unwrap_or(false);

        if is_control {
            bail!("Control variant weight is managed automatically and cannot be changed.");
        }
    }

    let total = total_non_control_weight(
        ctx.feature.as_ref().unwrap(),
        ctx.feature_patch.as_ref(),
        &variant_ref,
        new_weight,
    );

    if total > 100 {
        bail!("Total weight of non-control variants would be {total}%, exceeding 100%.");
    }

    stage::stage_weight(ctx.get_or_init_feature_patch(), &variant_ref, new_weight)?;
    index::rebuild(&mut ctx);
    Ok(())
}

/// Interactive fallback for `weight` with no arguments - the same weight-adjustment menu
/// segment overrides use ([`menu::adjust_weights`], via the shared row-builder
/// [`effective::weight_menu_rows`]), pre-filled with every non-control variant's current
/// effective weight, plus a trailing read-only row showing the control variant's
/// auto-balanced remainder.
/// On confirm, stages a weight update for every row whose value actually changed.
fn adjust_weights_interactively(ctx: &mut Connection) -> anyhow::Result<()> {
    let variants =
        effective::effective_variants(ctx.feature.as_ref().unwrap(), ctx.feature_patch.as_ref());
    let (non_control, mut rows, default_suffix) =
        effective::weight_menu_rows(&variants, |v| v.weight);

    if rows.is_empty() {
        bail!("No non-control variants to adjust. Use `VARIANT add` first.");
    }

    let confirmed = menu::adjust_weights("Adjust variant weights", &mut rows, &default_suffix)?;
    if !confirmed {
        return Ok(());
    }

    let mut staged_pos = 0usize;
    for (v, row) in non_control.iter().zip(rows.iter()) {
        let variant_ref = match v.id {
            Some(id) => VariantRef::Committed(id),
            None => {
                let r = VariantRef::Staged(staged_pos);
                staged_pos += 1;
                r
            }
        };
        if row.weight != v.weight {
            stage::stage_weight(ctx.get_or_init_feature_patch(), &variant_ref, row.weight)?;
        }
    }
    index::rebuild(ctx);
    Ok(())
}

/// Stage a deletion for the variant at the given display index.
///
/// Expected args: `<index>`
///
/// For committed variants, clears any pending SetValue/SetWeight ops for that id and
/// appends a Delete op. For staged additions, there's nothing committed to delete - the
/// pending Add op is discarded instead. Refuses to delete the control variant.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }

    let variant_ref = match args.get(1) {
        Some(idx) => index::resolve(idx.parse::<usize>()?, &ctx)?,
        None => bail!("No variant index provided."),
    };

    if let VariantRef::Committed(id) = &variant_ref {
        let is_control = ctx
            .feature
            .as_ref()
            .and_then(|f| f.variants.iter().find(|v| v.id == *id))
            .map(|v| v.is_control())
            .unwrap_or(false);

        if is_control {
            bail!("Control variant is managed automatically and cannot be deleted.");
        }
    }

    let variant_id = match variant_ref {
        VariantRef::Committed(id) => id,
        VariantRef::Staged(_) => {
            if let Some(pending) = ctx.feature_patch.as_mut() {
                stage::discard_feature_patch(pending, &variant_ref);
                index::rebuild(&mut ctx);
            } else {
                println!("No pending variant changes.");
            }
            return Ok(());
        }
    };

    // Find the variant's value before staging deletion, so we can clean up any
    // staged identity override that references it (overrides are keyed by value).
    let deleted_variant_value = ctx
        .feature
        .as_ref()
        .and_then(|f| f.variants.iter().find(|v| v.id == variant_id))
        .map(|v| v.value.clone());

    let ops = &mut ctx.get_or_init_feature_patch().variants;
    ops.retain(|op| {
        !matches!(op,
            VariantPatchOp::SetValue { id, .. } | VariantPatchOp::SetWeight { id, .. }
            if *id == variant_id
        )
    });

    println!("Staged: variant delete id={variant_id}");
    ops.push(VariantPatchOp::Delete { id: variant_id });

    // If there's a staged identity override pointing at this variant, remove it -
    // committing it after the variant is deleted would leave a dangling reference.
    if let Some(val) = deleted_variant_value {
        let feature_name = ctx.feature.as_ref().unwrap().name.clone();
        if let Some(patch) = ctx.identity_patch.as_mut() {
            let before = patch.overrides.len();
            patch
                .overrides
                .retain(|o| !(o.feature_name == feature_name && o.variant_value == val));
            if patch.overrides.len() < before {
                let identity = ctx
                    .identity
                    .as_ref()
                    .map(|i| i.value.as_str())
                    .unwrap_or("<unknown>");
                println!(
                    "Dropped staged override for '{val}' on identity '{identity}' (variant is being deleted)."
                );
            }
        }
    }

    index::rebuild(&mut ctx);
    Ok(())
}

/// Computes the total weight of all non-control variants, applying pending overrides and
/// substituting `new_weight` for the variant identified by `variant_ref`.
fn total_non_control_weight(
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

/// Generic helper that resolves the current value of a variant field, preferring any staged
/// override over the committed state. `extract_set` matches a specific Set* op for committed
/// variants; `extract_committed` reads the field from the committed variant; `extract_add`
/// reads the field from a staged Add op.
fn current_variant_field<T, F, G, H>(
    variant_ref: &VariantRef,
    ctx: &Connection,
    extract_set: F,
    extract_committed: G,
    extract_add: H,
) -> T
where
    T: Default,
    F: Fn(&VariantPatchOp, i32) -> Option<T>,
    G: Fn(&Variant) -> T,
    H: Fn(&VariantPatchOp) -> Option<T>,
{
    match variant_ref {
        VariantRef::Committed(id) => {
            let staged = ctx
                .feature_patch
                .as_ref()
                .and_then(|p| p.variants.iter().rev().find_map(|op| extract_set(op, *id)));
            staged.unwrap_or_else(|| {
                ctx.feature
                    .as_ref()
                    .and_then(|f| f.variants.iter().find(|v| v.id == *id))
                    .map(extract_committed)
                    .unwrap_or_default()
            })
        }
        VariantRef::Staged(pos) => ctx
            .feature_patch
            .as_ref()
            .and_then(|p| p.variants.iter().filter_map(extract_add).nth(*pos))
            .unwrap_or_default(),
    }
}

/// Returns the current weight for a variant, used as a base when applying relative weight
/// changes. For committed variants, prefers any already-staged `SetWeight` op; for staged
/// (Add) variants, returns the weight from the pending `Add` op.
fn current_variant_weight(variant_ref: &VariantRef, ctx: &Connection) -> i16 {
    current_variant_field(
        variant_ref,
        ctx,
        |op, id| match op {
            VariantPatchOp::SetWeight { id: sid, weight } if *sid == id => Some(*weight as i16),
            _ => None,
        },
        |v| v.weight as i16,
        |op| match op {
            VariantPatchOp::Add { weight, .. } => Some(*weight as i16),
            _ => None,
        },
    )
}

/// Returns the current [`VariantValue`] for a variant, used as a type fallback when staging
/// a value change. For committed variants, prefers any already-staged `SetValue` op; for
/// staged (Add) variants, returns the value from the pending `Add` op.
fn current_variant_value(variant_ref: &VariantRef, ctx: &Connection) -> VariantValue {
    current_variant_field(
        variant_ref,
        ctx,
        |op, id| match op {
            VariantPatchOp::SetValue { id: oid, value } if *oid == id => Some(value.clone()),
            _ => None,
        },
        |v| v.value.clone(),
        |op| match op {
            VariantPatchOp::Add { value, .. } => Some(value.clone()),
            _ => None,
        },
    )
}
