//! REPL command handlers for variant management.
//!
//! Each public function corresponds to a `VARIANT <op>` command:
//!
//! | Command            | Handler    | Description                                      |
//! |--------------------|------------|--------------------------------------------------|
//! | `VARIANT list`     | [`list`]   | Print variants and rebuild the positional index. |
//! | `VARIANT add`      | [`add`]    | Stage a new variant addition.                    |
//! | `VARIANT value`    | [`value`]  | Stage a value change for an existing variant.    |
//! | `VARIANT weight`   | [`weight`] | Stage a weight change for an existing variant.   |
//! | `VARIANT discard`  | [`discard`]| Drop staged ops for a single variant.            |
//! | `VARIANT delete`   | [`delete`] | Stage a variant deletion.                        |
//!
//! All mutations are accumulated in [`Connection::pending`] as a [`FeaturePatch`] and
//! only sent to the API when the user runs `COMMIT`.

use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Feature, FeatureValue, Variant,
    payload::{FeaturePatch, VariantPatchOp},
};

use crate::handlers::{
    open_in_editor,
    internal::{index, stage, effectives as effective},
};
use crate::printer::tabular::{VariantRow, bar, variant_list};

/// List variants for the current feature, overlaying any pending staged changes.
///
/// Committed variants are shown first (sorted ascending by id), followed by staged
/// additions. Modified, deleted, and auto-adjusted rows are colour-coded. Also
/// rebuilds the positional variant index used by other commands.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    let feature = ctx
        .feature
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No feature name provided."))?;
    let ops: &[VariantPatchOp] = ctx
        .feature_patch
        .as_ref()
        .map(|p| p.variants.as_slice())
        .unwrap_or_default();

    // Pre-compute adjusted control weight once; it is constant across all variants in the loop.
    let adjusted_control_weight: Option<u8> = if !ops.is_empty() {
        let non_control_total = total_non_control_weight(
            feature,
            ctx.feature_patch.as_ref(),
            &VariantRef::Staged(usize::MAX),
            0,
        );
        let adjusted = 100u32.saturating_sub(non_control_total) as u8;
        feature
            .variants
            .iter()
            .find(|v| v.is_control())
            .and_then(|c| (adjusted != c.weight).then_some(adjusted))
    } else {
        None
    };

    let eff = effective::effective_variants(feature, ctx.feature_patch.as_ref());

    let mut rows: Vec<VariantRow> = eff
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_staged_add)
        .map(|(i, e)| {
            let idx_str = if e.is_control {
                format!("{}★", i + 1)
            } else {
                (i + 1).to_string()
            };

            if e.is_deleted {
                VariantRow {
                    index: idx_str.dimmed().to_string(),
                    weight: bar(e.weight, 10).dimmed().to_string(),
                    value: e.value.to_string().dimmed().to_string(),
                    stage: Some("deleted".red().to_string()),
                }
            } else {
                let weight = if e.is_control {
                    adjusted_control_weight.unwrap_or(e.weight)
                } else {
                    e.weight
                };
                let is_modified = e.value_modified
                    || e.weight_modified
                    || (e.is_control && adjusted_control_weight.is_some());
                let label = if e.value_modified || e.weight_modified {
                    "modified"
                } else {
                    "adjusted"
                };
                if is_modified {
                    VariantRow {
                        index: idx_str.yellow().to_string(),
                        weight: bar(weight, 10).yellow().to_string(),
                        value: e.value.to_string().yellow().to_string(),
                        stage: Some(label.yellow().to_string()),
                    }
                } else {
                    VariantRow {
                        index: idx_str,
                        weight: bar(weight, 10),
                        value: e.value.to_string(),
                        stage: None,
                    }
                }
            }
        })
        .collect();

    let committed_count = eff.iter().filter(|e| !e.is_staged_add).count();
    for (staged_pos, e) in eff.iter().filter(|e| e.is_staged_add).enumerate() {
        rows.push(VariantRow {
            index: (committed_count + staged_pos + 1)
                .to_string()
                .green()
                .to_string(),
            weight: bar(e.weight, 10).green().to_string(),
            value: e.value.to_string().green().to_string(),
            stage: Some("added".green().to_string()),
        });
    }

    variant_list(rows);

    let staged_adds_count = eff.iter().filter(|e| e.is_staged_add).count();
    ctx.variant_index = eff
        .iter()
        .filter(|e| !e.is_staged_add)
        .filter_map(|e| e.id.map(VariantRef::Committed))
        .chain((0..staged_adds_count).map(VariantRef::Staged))
        .collect();

    Ok(())
}

/// Stage a new variant addition with a given weight and value.
///
/// Expects args: `<weight> [value]`
///
/// If value is omitted, opens `$EDITOR` for interactive input. Fails if the
/// new weight would push total non-control weight over 100%.
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

    let fv: FeatureValue = value
        .parse()
        .unwrap_or_else(|_| FeatureValue::build(&value));

    let feature = ctx.feature.as_ref().unwrap();
    if feature.variants.iter().any(|v| v.value == fv)
        || ctx.feature_patch.as_ref().map_or(false, |p| {
            p.variants
                .iter()
                .any(|op| matches!(op, VariantPatchOp::Add { value, .. } if *value == fv))
        })
    {
        bail!("A variant with this value already exists for this feature.");
    }

    println!("Staged: variant add weight={weight} value={value}");

    ctx.get_or_init_pending()
        .variants
        .push(VariantPatchOp::Add { value: fv, weight });

    index::rebuild(&mut ctx);
    Ok(())
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
    let fv = raw
        .parse::<FeatureValue>()
        .unwrap_or_else(|_| current.clone_with(raw.trim()));

    let feature = ctx.feature.as_ref().unwrap();
    let duplicate = feature.variants.iter().any(|v| {
        v.value == fv && !matches!(&variant_ref, VariantRef::Committed(id) if *id == v.id)
    }) || ctx.feature_patch.as_ref().map_or(false, |p| {
        p.variants.iter().enumerate().any(|(i, op)| match op {
            VariantPatchOp::Add { value, .. } => {
                *value == fv && !matches!(&variant_ref, VariantRef::Staged(pos) if *pos == i)
            }
            VariantPatchOp::SetValue { id, value } => {
                *value == fv && !matches!(&variant_ref, VariantRef::Committed(vid) if *vid == *id)
            }
            _ => false,
        })
    });
    if duplicate {
        bail!("A variant with this value already exists for this feature.");
    }

    let old_value = current.to_string();
    let new_value = fv.to_string();

    stage::stage_value(ctx.get_or_init_pending(), &variant_ref, fv)?;

    // Keep any staged identity override in sync: if it was pointing at the old value,
    // update it to the new value so it doesn't become stale after commit.
    if old_value != new_value {
        let feature_name = ctx.feature.as_ref().unwrap().name.clone();
        if let Some(patch) = ctx.identity_patch.as_mut() {
            for o in patch.overrides.iter_mut() {
                if o.feature_name == feature_name && o.variant_value == old_value {
                    o.variant_value = new_value.clone();
                    println!("Updated staged override value: '{old_value}' → '{new_value}'");
                }
            }
        }
    }

    index::rebuild(&mut ctx);
    Ok(())
}

/// Stage a weight change for an existing variant identified by its display index.
///
/// Expected args: `[+/-]<weight>`
///
/// Weight may be an absolute value (e.g. `30`) or a relative change prefixed with `+` or `-`
/// (e.g. `+5` adds 5 to the current weight, `-3` subtracts 3). Refuses to change the control
/// variant's weight (it is auto-adjusted) and rejects values that would push total non-control
/// weight over 100%.
pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => index::resolve(idx.parse::<usize>()?, &ctx)?,
        None => bail!("No variant index provided."),
    };
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

    stage::stage_weight(ctx.get_or_init_pending(), &variant_ref, new_weight)?;
    index::rebuild(&mut ctx);
    Ok(())
}

/// Discard a single pending change for the variant at the given display index.
///
/// Expected args: `<index>`
///
/// For committed variants removes any SetValue/SetWeight/Delete ops for that id.
/// For staged additions removes the Add op entirely.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => index::resolve(idx.parse::<usize>()?, &ctx)?,
        None => bail!("No variant index provided. Use an index or 'all'."),
    };
    let pending = match ctx.feature_patch.as_mut() {
        Some(p) => p,
        None => {
            println!("No pending variant changes.");
            return Ok(());
        }
    };

    stage::discard_feature_patch(pending, &variant_ref);
    index::rebuild(&mut ctx);
    Ok(())
}

/// Stage a deletion for the variant at the given display index.
///
/// Expected args: `<index>`
///
/// For committed variants, clears any pending SetValue/SetWeight ops for that id and
/// appends a Delete op. For staged additions, delegates to [`discard`] to remove the
/// Add op entirely. Refuses to delete the control variant.
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
            drop(ctx);
            return discard(args, session);
        }
    };

    // Find the variant's value before staging deletion, so we can clean up any
    // staged identity override that references it (overrides are keyed by value string).
    let deleted_variant_value = ctx
        .feature
        .as_ref()
        .and_then(|f| f.variants.iter().find(|v| v.id == variant_id))
        .map(|v| v.value.to_string());

    let ops = &mut ctx.get_or_init_pending().variants;
    ops.retain(|op| {
        !matches!(op,
            VariantPatchOp::SetValue { id, .. } | VariantPatchOp::SetWeight { id, .. }
            if *id == variant_id
        )
    });

    println!("Staged: variant delete id={variant_id}");
    ops.push(VariantPatchOp::Delete { id: variant_id });

    // If there's a staged identity override pointing at this variant, remove it —
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
                    .map(|v| extract_committed(v))
                    .unwrap_or_default()
            })
        }
        VariantRef::Staged(pos) => ctx
            .feature_patch
            .as_ref()
            .and_then(|p| p.variants.iter().filter_map(|op| extract_add(op)).nth(*pos))
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

/// Returns the current [`FeatureValue`] for a variant, used as a type fallback when staging
/// a value change. For committed variants, prefers any already-staged `SetValue` op; for
/// staged (Add) variants, returns the value from the pending `Add` op.
fn current_variant_value(variant_ref: &VariantRef, ctx: &Connection) -> FeatureValue {
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
