use anyhow::bail;
use colored::Colorize;
use flagrant_client::connection::{Connection, VariantRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{FeatureValue, Variant, payload::VariantPatchOp};

use crate::printer::tabular::{VariantRow, bar, variant_list};

pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("No feature name provided.");
    }

    // SAFETY: checked above; borrow only the two fields we need so that the
    // immutable borrows of `feature` and `pending` end before the mutable
    // write to `ctx.variant_index` further below.
    let variants: &[Variant] = &ctx.feature.as_ref().unwrap().variants;
    let default_value = ctx.feature.as_ref().unwrap().get_default_value();
    let ops: &[VariantPatchOp] = ctx
        .pending
        .as_ref()
        .map(|p| p.variants.as_slice())
        .unwrap_or_default();

    // Committed variants go first (sorted ascending by id).
    let mut sorted_variants: Vec<&Variant> = variants.iter().collect();
    sorted_variants.sort_by_key(|v| v.id);

    let mut var_index = sorted_variants
        .iter()
        .map(|v| VariantRef::Committed(v.id))
        .collect();

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

    // Track which committed variant ids are deleted.
    let deleted_ids: std::collections::HashSet<i32> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::Delete { id } => Some(*id),
            _ => None,
        })
        .collect();

    // Collect value/weight overrides by id.
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

    // Staged Add ops - collect (ops-vec-index, value, weight) in staging order.
    let staged_adds: Vec<(usize, &str, u8)> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| match op {
            VariantPatchOp::Add { value, weight } => Some((i, value.as_str(), *weight)),
            _ => None,
        })
        .collect();

    let mut rows: Vec<VariantRow> = Vec::new();

    // Committed variants (with pending modifications overlaid).
    for (display_idx, var) in sorted_variants.iter().enumerate() {
        let is_deleted = deleted_ids.contains(&var.id);
        let new_value = value_overrides.get(&var.id).and_then(|v| v.as_deref());
        let new_weight = weight_overrides.get(&var.id).and_then(|w| *w);
        let is_modified = new_value.is_some() || new_weight.is_some();

        let idx_str = (display_idx + 1).to_string();
        let id_str = var.id.to_string();
        let weight = new_weight.unwrap_or(var.weight);
        let weight_str = bar(weight, 10);
        let value_str = match new_value {
            Some(v) => var.value.clone_with(v).to_string(),
            None => var.value.to_string(),
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

    // Staged additions (no real id yet).
    let committed_count = sorted_variants.len();
    for (staged_pos, (_, value, weight)) in staged_adds.iter().enumerate() {
        let display_idx = committed_count + staged_pos + 1;
        let fv = value
            .parse::<FeatureValue>()
            .unwrap_or_else(|_| default_value.clone_with(value));

        rows.push(VariantRow {
            index: display_idx.to_string().green().to_string(),
            id: "-".green().to_string(),
            weight: bar(*weight, 10).green().to_string(),
            value: fv.to_string().green().to_string(),
            state: Some("staged".green().to_string()),
        });
    }

    variant_list(rows);

    // Build the positional index: committed first (by id), then staged.
    for staged_pos in 0..staged_adds.len() {
        var_index.push(VariantRef::Staged(staged_pos));
    }
    ctx.variant_index = var_index;
    Ok(())
}

/// Resolve a 1-based display index from the last `var list` output to a VariantRef.
fn resolve_index(
    ctx: &flagrant_client::connection::Connection,
    raw: &Arg,
) -> anyhow::Result<VariantRef> {
    let idx: usize = raw.parse::<usize>()?;
    if idx == 0 || idx > ctx.variant_index.len() {
        bail!(
            "Index {} out of range (1–{}). Run `var list` to refresh.",
            idx,
            ctx.variant_index.len()
        );
    }
    Ok(ctx.variant_index[idx - 1].clone())
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

    ctx.get_or_init_pending()
        .variants
        .push(VariantPatchOp::Add {
            value: value.clone(),
            weight,
        });
    ctx.variant_index.clear();
    println!("Staged: variant add weight={weight} value={value}");
    Ok(())
}

pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(&ctx, idx)?,
        None => bail!("No variant index provided."),
    };
    let raw = match args.get(2) {
        Some(v) => v.to_string(),
        None => bail!("No value provided."),
    };

    let variant_id = match variant_ref {
        VariantRef::Committed(id) => id,
        VariantRef::Staged(_) => bail!(
            "Cannot set value on a staged (not yet committed) variant. Discard and re-add it instead."
        ),
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    if let Some(op) = ops
        .iter_mut()
        .find(|op| matches!(op, VariantPatchOp::SetValue { id, .. } if *id == variant_id))
    {
        *op = VariantPatchOp::SetValue {
            id: variant_id,
            value: raw.clone(),
        };
    } else {
        ops.push(VariantPatchOp::SetValue {
            id: variant_id,
            value: raw.clone(),
        });
    }
    ctx.variant_index.clear();
    println!("Staged: variant value id={variant_id} value={raw}");
    Ok(())
}

pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(&ctx, idx)?,
        None => bail!("No variant index provided."),
    };
    let new_weight = match args.get(2) {
        Some(w) => w.parse::<u8>()?,
        None => bail!("No weight provided."),
    };
    if !(0..=100).contains(&new_weight) {
        bail!("Variant weight should be positive number in range of <0, 100>.")
    }

    let variant_id = match variant_ref {
        VariantRef::Committed(id) => id,
        VariantRef::Staged(_) => bail!(
            "Cannot set weight on a staged (not yet committed) variant. Discard and re-add it instead."
        ),
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    if let Some(op) = ops
        .iter_mut()
        .find(|op| matches!(op, VariantPatchOp::SetWeight { id, .. } if *id == variant_id))
    {
        *op = VariantPatchOp::SetWeight {
            id: variant_id,
            weight: new_weight,
        };
    } else {
        ops.push(VariantPatchOp::SetWeight {
            id: variant_id,
            weight: new_weight,
        });
    }
    ctx.variant_index.clear();
    println!("Staged: variant weight id={variant_id} weight={new_weight}");
    Ok(())
}

pub fn del(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(&ctx, idx)?,
        None => bail!("No variant index provided."),
    };

    let variant_id = match variant_ref {
        VariantRef::Committed(id) => id,
        VariantRef::Staged(_) => {
            bail!("Cannot delete a staged variant. Use `var discard <index>` to remove it.")
        }
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    ops.retain(|op| {
        !matches!(op,
            VariantPatchOp::SetValue { id, .. } | VariantPatchOp::SetWeight { id, .. }
            if *id == variant_id
        )
    });
    ops.push(VariantPatchOp::Delete { id: variant_id });
    ctx.variant_index.clear();
    println!("Staged: variant delete id={variant_id}");
    Ok(())
}

/// Discard a single pending change for the variant at the given display index.
/// For committed variants: removes any SetValue/SetWeight/Delete ops for that id.
/// For staged additions: removes the Add op entirely.
pub fn discard(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_ref = match args.get(1) {
        Some(idx) => resolve_index(&ctx, idx)?,
        None => bail!("No variant index provided."),
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
            // Find the nth Add op and remove it.
            let mut add_count = 0usize;
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

    ctx.variant_index.clear();
    Ok(())
}
