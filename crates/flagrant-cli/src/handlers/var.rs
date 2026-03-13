use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    FeatureValue, Variant,
    payload::{FeaturePatch, VariantPatchOp},
};

use crate::printer::tabular::{VariantRow, bar, variant_list};

pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("No feature name provided.");
    }

    let default = ctx.feature.as_ref().unwrap().get_default_value();
    let variants: &[Variant] = &ctx.feature.as_ref().unwrap().variants;
    let ops: &[VariantPatchOp] = ctx
        .pending
        .as_ref()
        .map(|p| p.variants.as_slice())
        .unwrap_or_default();

    // committed variants go first (sorted ascending by id).
    let mut sorted_variants: Vec<&Variant> = variants.iter().collect();
    let committed_count = sorted_variants.len();

    sorted_variants.sort_by_key(|v| v.id);

    let mut var_index = sorted_variants
        .iter()
        .map(|v| VariantRef::Committed(v.id))
        .collect();

    // short circuit - if no modifications were added simply list current variants
    if ops.is_empty() {
        let rows: Vec<VariantRow> = sorted_variants
            .iter()
            .enumerate()
            .map(|(i, var)| VariantRow::committed(i + 1, var))
            .collect();

        variant_list(rows);

        ctx.variant_index = var_index;
        return Ok(());
    }

    // track which committed variant ids are deleted
    let deleted_ids: std::collections::HashSet<i32> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::Delete { id } => Some(*id),
            _ => None,
        })
        .collect();

    // collect value/weight overrides by id
    let mut value_overrides: std::collections::HashMap<i32, Option<String>> =
        std::collections::HashMap::new();
    let mut weight_overrides: std::collections::HashMap<i32, Option<u8>> =
        std::collections::HashMap::new();

    for op in ops {
        match op {
            VariantPatchOp::SetValue { id, value } => {
                value_overrides.insert(*id, Some(value.clone()));
            }
            VariantPatchOp::SetWeight { id, weight } => {
                weight_overrides.insert(*id, Some(*weight));
            }
            _ => {}
        }
    }

    // staged Add ops - collect (ops-vec-index, value, weight) in staging order
    let staged_adds: Vec<(usize, &str, u8)> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| match op {
            VariantPatchOp::Add { value, weight } => Some((i, value.as_str(), *weight)),
            _ => None,
        })
        .collect();

    let mut rows: Vec<VariantRow> = Vec::new();

    // committed variants (with pending modifications overlaid).
    for (display_idx, var) in sorted_variants.iter().enumerate() {
        let is_deleted = deleted_ids.contains(&var.id);
        let new_value = value_overrides.get(&var.id).and_then(|v| v.as_deref());
        let new_weight = weight_overrides.get(&var.id).and_then(|w| *w);
        let is_modified = new_value.is_some() || new_weight.is_some();

        // for the control variant, compute the auto-adjusted weight based on pending ops.
        // note, control variant cannot have its own pending modification - it's always auto-adjusted.
        let adjusted_control_weight: Option<u8> = if var.is_control() {
            let non_control_total = total_non_control_weight(
                ctx.feature.as_ref().unwrap(),
                ctx.pending.as_ref(),
                &VariantRef::Staged(usize::MAX), // no substitution – use all pending weights as-is
                0,
            );
            let adjusted = 100u32.saturating_sub(non_control_total) as u8;
            if adjusted != var.weight {
                Some(adjusted)
            } else {
                None
            }
        } else {
            None
        };

        let id_str = var.id.to_string();
        let weight = new_weight.or(adjusted_control_weight).unwrap_or(var.weight);
        let weight_str = bar(weight, 10);
        let value_str = match new_value {
            Some(v) => var.value.clone_with(v).to_string(),
            None => var.value.to_string(),
        };
        let idx_str = if var.is_control() {
            format!("{}★", display_idx + 1)
        } else {
            (display_idx + 1).to_string()
        };

        rows.push(if is_deleted {
            VariantRow {
                index: idx_str.dimmed().to_string(),
                id: id_str.dimmed().to_string(),
                weight: weight_str.dimmed().to_string(),
                value: value_str.dimmed().to_string(),
                state: Some("deleted".red().to_string()),
            }
        } else if is_modified {
            VariantRow {
                index: idx_str.yellow().to_string(),
                id: id_str.yellow().to_string(),
                weight: weight_str.yellow().to_string(),
                value: value_str.yellow().to_string(),
                state: Some("modified".yellow().to_string()),
            }
        } else if adjusted_control_weight.is_some() {
            VariantRow {
                index: idx_str.yellow().to_string(),
                id: id_str.yellow().to_string(),
                weight: weight_str.yellow().to_string(),
                value: value_str.yellow().to_string(),
                state: Some("adjusted".yellow().to_string()),
            }
        } else {
            VariantRow {
                index: idx_str,
                id: id_str,
                weight: weight_str,
                value: value_str,
                state: Some(String::new()),
            }
        });
    }

    for (staged_pos, (_, value, weight)) in staged_adds.iter().enumerate() {
        let display_idx = committed_count + staged_pos + 1;
        let fv = value
            .parse::<FeatureValue>()
            .unwrap_or_else(|_| default.clone_with(value));

        rows.push(VariantRow {
            index: display_idx.to_string().green().to_string(),
            id: "-".green().to_string(),
            weight: bar(*weight, 10).green().to_string(),
            value: fv.to_string().green().to_string(),
            state: Some("staged".green().to_string()),
        });
    }

    variant_list(rows);

    // Build the positional index: committed first (by id), then staged
    for staged_pos in 0..staged_adds.len() {
        var_index.push(VariantRef::Staged(staged_pos));
    }
    ctx.variant_index = var_index;
    Ok(())
}

pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let weight = match args.get(1) {
        Some(w) => w.parse::<u8>()?,
        None => bail!("No weight provided."),
    };
    let value = args.get(2).map(|a| a.to_string()).unwrap_or_default();

    if !(0..=100).contains(&weight) {
        bail!("Variant weight should be positive number in range of <0, 100>.")
    }

    let total = weight as u32
        + total_non_control_weight(
            ctx.feature.as_ref().unwrap(),
            ctx.pending.as_ref(),
            &VariantRef::Staged(usize::MAX),
            weight,
        );
    if total > 100 {
        bail!("Total weight of non-control variants would be {total}%, exceeding 100%.");
    }

    ctx.get_or_init_pending()
        .variants
        .push(VariantPatchOp::Add {
            value: value.clone(),
            weight,
        });

    println!("Staged: variant add weight={weight} value={value}");
    rebuild_index(&mut ctx);
    Ok(())
}

pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(idx, &ctx)?,
        None => bail!("No variant index provided."),
    };
    let raw = match args.get(2) {
        Some(v) => v.to_string(),
        None => bail!("No value provided."),
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    match variant_ref {
        VariantRef::Committed(id) => {
            if let Some(op) = ops
                .iter_mut()
                .find(|op| matches!(op, VariantPatchOp::SetValue { id: oid, .. } if *oid == id))
            {
                *op = VariantPatchOp::SetValue {
                    id,
                    value: raw.clone(),
                };
            } else {
                ops.push(VariantPatchOp::SetValue {
                    id,
                    value: raw.clone(),
                });
            }
            println!("Staged: variant value id={id} value={raw}");
        }
        VariantRef::Staged(staged_pos) => {
            let add_op = ops
                .iter_mut()
                .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                .nth(staged_pos);
            match add_op {
                Some(VariantPatchOp::Add { value, .. }) => {
                    *value = raw.clone();
                    println!("Updated staged variant value to {raw}");
                }
                _ => bail!("Staged variant not found."),
            }
        }
    }

    rebuild_index(&mut ctx);
    Ok(())
}

pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(idx, &ctx)?,
        None => bail!("No variant index provided."),
    };
    let new_weight = match args.get(2) {
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
        ctx.pending.as_ref(),
        &variant_ref,
        new_weight,
    );
    if total > 100 {
        bail!("Total weight of non-control variants would be {total}%, exceeding 100%.");
    }

    let ops = &mut ctx.get_or_init_pending().variants;
    match variant_ref {
        VariantRef::Committed(id) => {
            if let Some(op) = ops
                .iter_mut()
                .find(|op| matches!(op, VariantPatchOp::SetWeight { id: oid, .. } if *oid == id))
            {
                *op = VariantPatchOp::SetWeight {
                    id,
                    weight: new_weight,
                };
            } else {
                ops.push(VariantPatchOp::SetWeight {
                    id,
                    weight: new_weight,
                });
            }
            println!("Staged: variant weight id={id} weight={new_weight}");
        }
        VariantRef::Staged(staged_pos) => {
            let add_op = ops
                .iter_mut()
                .filter(|op| matches!(op, VariantPatchOp::Add { .. }))
                .nth(staged_pos);
            match add_op {
                Some(VariantPatchOp::Add { weight, .. }) => {
                    *weight = new_weight;
                    println!("Updated staged variant weight to {new_weight}");
                }
                _ => bail!("Staged variant not found."),
            }
        }
    }

    rebuild_index(&mut ctx);
    Ok(())
}

pub fn del(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(idx, &ctx)?,
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

    let ops = &mut ctx.get_or_init_pending().variants;
    ops.retain(|op| {
        !matches!(op,
            VariantPatchOp::SetValue { id, .. } | VariantPatchOp::SetWeight { id, .. }
            if *id == variant_id
        )
    });
    println!("Staged: variant delete id={variant_id}");
    ops.push(VariantPatchOp::Delete { id: variant_id });

    rebuild_index(&mut ctx);
    Ok(())
}

/// Discard a single pending change for the variant at the given display index:
///  - for committed variants: removes any SetValue/SetWeight/Delete ops for that id
///  - for staged additions: removes the Add op entirely
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }

    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(idx, &ctx)?,
        None => bail!("No variant index provided. Use an index or 'all'."),
    };

    let pending = match ctx.pending.as_mut() {
        Some(p) => p,
        None => {
            println!("No pending variant changes.");
            return Ok(());
        }
    };

    match variant_ref {
        VariantRef::Committed(id) => {
            let before = pending.variants.len();
            pending.variants.retain(|op| {
                !matches!(op,
                    VariantPatchOp::SetValue { id: oid, .. }
                    | VariantPatchOp::SetWeight { id: oid, .. }
                    | VariantPatchOp::Delete { id: oid }
                    if *oid == id
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
                    if add_count == staged_pos {
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

    rebuild_index(&mut ctx);
    Ok(())
}

/// Computes the total weight of all non-control variants, applying pending overrides and
/// substituting `new_weight` for the variant identified by `variant_ref`.
fn total_non_control_weight(
    feature: &flagrant_types::Feature,
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

/// Resolve a 1-based display index from the last `VARIANT list` output to a VariantRef.
fn resolve_index(raw: &Arg, ctx: &Connection) -> anyhow::Result<VariantRef> {
    let idx: usize = raw.parse::<usize>()?;

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
fn rebuild_index(ctx: &mut Connection) {
    let variants = ctx
        .feature
        .as_ref()
        .map(|f| f.variants.as_slice())
        .unwrap_or_default();
    let staged_count = ctx
        .pending
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
