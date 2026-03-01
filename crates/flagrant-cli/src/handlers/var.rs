use anyhow::bail;
use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout};
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{FeatureValue, Variant, payload::VariantPatchOp};

use crate::printer::tabular::{Tabular, bar};

pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();

    let feature = match ctx.feature.as_ref() {
        Some(f) => f,
        None => bail!("No feature name provided."),
    };

    // If there are no pending variant ops, use the simple existing renderer.
    let has_pending_variant_ops = ctx
        .pending
        .as_ref()
        .map(|p| !p.variants.is_empty())
        .unwrap_or(false);

    if !has_pending_variant_ops {
        Variant::list(&feature.variants);
        return Ok(());
    }

    // Build a merged view: start from committed variants, apply pending ops on top.
    // Each row carries: (id_label, weight_label, value_label, state_label)
    // State: "committed", "modified", "deleted", "staged"
    let ops = &ctx.pending.as_ref().unwrap().variants;

    // Track which committed variant ids are deleted
    let deleted_ids: std::collections::HashSet<i32> = ops
        .iter()
        .filter_map(|op| match op {
            VariantPatchOp::Delete { id } => Some(*id),
            _ => None,
        })
        .collect();

    // Collect value/weight overrides by id
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

    let mut rows: Vec<[String; 4]> = Vec::new();

    // Committed variants (with pending modifications overlaid)
    for var in &feature.variants {
        let is_deleted = deleted_ids.contains(&var.id);
        let new_value = value_overrides.get(&var.id).and_then(|v| v.as_deref());
        let new_weight = weight_overrides.get(&var.id).and_then(|w| *w);
        let is_modified = new_value.is_some() || new_weight.is_some();

        let id_str = var.id.to_string();
        let weight = new_weight.unwrap_or(var.weight);
        let weight_str = bar(weight, 10);
        let value_str = match new_value {
            Some(v) => var.value.clone_with(v).to_string(),
            None => var.value.to_string(),
        };

        let (id_disp, weight_disp, value_disp, state_disp) = if is_deleted {
            (
                id_str.dimmed().to_string(),
                weight_str.dimmed().to_string(),
                value_str.dimmed().to_string(),
                "deleted".red().to_string(),
            )
        } else if is_modified {
            (
                id_str.yellow().to_string(),
                weight_str.yellow().to_string(),
                value_str.yellow().to_string(),
                "modified".yellow().to_string(),
            )
        } else {
            (id_str, weight_str, value_str, String::new())
        };

        rows.push([id_disp, weight_disp, value_disp, state_disp]);
    }

    // Staged additions (no real id yet)
    for op in ops {
        if let VariantPatchOp::Add { value, weight } = op {
            let fv = value
                .parse::<FeatureValue>()
                .unwrap_or_else(|_| feature.get_default_value().clone_with(value));
            rows.push([
                "new".green().to_string(),
                bar(*weight, 10).green().to_string(),
                fv.to_string().green().to_string(),
                "staged".green().to_string(),
            ]);
        }
    }

    FancyTable::create(FancyTableOpts::default())
        .add_column_named_with_align("ID".into(), Layout::Fixed(10), Align::Left)
        .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
        .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
        .add_column_named_with_align("STATE".into(), Layout::Fixed(10), Align::Left)
        .width(100)
        .build()
        .render(rows);

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

    ctx.get_or_init_pending()
        .variants
        .push(VariantPatchOp::Add {
            value: value.clone(),
            weight,
        });
    println!("Staged: variant add weight={weight} value={value}");
    Ok(())
}

pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_id = match args.get(1) {
        Some(id) => id.parse::<i32>()?,
        None => bail!("No variant-id provided."),
    };
    let raw = match args.get(2) {
        Some(v) => v.to_string(),
        None => bail!("No value provided."),
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    // Update in-place if a SetValue for this id is already buffered
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
    println!("Staged: variant value id={variant_id} value={raw}");
    Ok(())
}

pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_id = match args.get(1) {
        Some(id) => id.parse::<i32>()?,
        None => bail!("No variant-id provided."),
    };
    let new_weight = match args.get(2) {
        Some(w) => w.parse::<u8>()?,
        None => bail!("No weight provided."),
    };
    if !(0..=100).contains(&new_weight) {
        bail!("Variant weight should be positive number in range of <0, 100>.")
    }

    let ops = &mut ctx.get_or_init_pending().variants;
    // Update in-place if a SetWeight for this id is already buffered
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
    println!("Staged: variant weight id={variant_id} weight={new_weight}");
    Ok(())
}

pub fn del(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();

    if ctx.feature.is_none() {
        bail!("Not within a feature context.");
    }
    let variant_id = match args.get(1) {
        Some(id) => id.parse::<i32>()?,
        None => bail!("No variant-id provided."),
    };

    let ops = &mut ctx.get_or_init_pending().variants;
    // Remove any buffered SetValue/SetWeight for this id — they're pointless if deleting
    ops.retain(|op| {
        !matches!(op,
            VariantPatchOp::SetValue { id, .. } | VariantPatchOp::SetWeight { id, .. }
            if *id == variant_id
        )
    });
    ops.push(VariantPatchOp::Delete { id: variant_id });
    println!("Staged: variant delete id={variant_id}");
    Ok(())
}
