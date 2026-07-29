//! REPL command handlers for segment rule management.
//!
//! | Command                              | Handler      | Description                                        |
//! |--------------------------------------|--------------|----------------------------------------------------|
//! | `RULE add <label> <driver> <cmp> <v>`| [`add`]      | Stage a new rule on a group in the current segment.|
//! | `RULE delete <label> <index>`        | [`delete`]   | Stage a rule deletion by 1-based index.            |
//! | `RULE show <label> <index>`          | [`show`]     | Print details of a single rule within a group.     |
//! | `RULE value <label> <index> [v]`     | [`value`]    | Stage a value change for an existing rule.         |
//! | `RULE comparator <label> <index> [c]`| [`comparator`] | Stage a comparator change for an existing rule.  |

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Comparator, SegmentDriver, SegmentRule,
    payload::{SegmentPatch, SegmentPatchOp},
};
use strum::IntoEnumIterator;

use crate::{
    handlers::internal::{extract_single_value, open_in_editor},
    printer::tabular::Tabular,
};

/// Print details of a single rule within a group, overlaying any staged changes.
///
/// Expected args: `<group-label> <rule-index>`
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let label = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Missing group label."))?;
    let index_str = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("Missing rule index."))?;
    let index: usize = index_str.parse::<usize>().map_err(|_| {
        anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
    })?;
    if index == 0 {
        bail!("Rule index is 1-based; use 1 for the first rule.");
    }

    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    let rule = group.rules.get(index - 1).ok_or_else(|| {
        anyhow::anyhow!(
            "No rule at index {index} in [{label}] (has {} rule(s)).",
            group.rules.len()
        )
    })?;

    rule.display(
        ctx.segment_patch.as_ref().filter(|p| !p.is_empty()),
        &(label.to_string(), index),
    );
    Ok(())
}

/// Stage a rule addition on a group in the current segment.
///
/// Expected args: `<group-label> <driver> <comparator> <value>`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let label = args.get(1).ok_or_else(|| {
        anyhow::anyhow!(
            "Missing group label. Expected: RULE add <group-label> <driver> <comparator> <value>"
        )
    })?;
    let driver_str = args.get(2).ok_or_else(|| {
        anyhow::anyhow!("Missing driver. Expected: identity, environment, trait:<name>")
    })?;
    let comparator_str = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("Missing comparator."))?;
    let value = args
        .get(4)
        .ok_or_else(|| anyhow::anyhow!("Missing value."))?;

    let driver = parse_driver(driver_str)?;
    let comparator = parse_comparator(comparator_str)?;

    let mut ctx = session.context.write().unwrap();
    if ctx.segment.is_none() {
        bail!("Not in a segment context. Use `SEGMENT use <name>` first.");
    }
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::AddRule {
            group_label: label.to_string(),
            driver,
            comparator,
            value: value.to_string(),
        });
    println!("Staged: add rule to [{}]", label);
    Ok(())
}

/// Stage a rule deletion by 1-based index within a group.
///
/// Expected args: `<group-label> <rule-index>`
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;

    let label = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Missing group label."))?;
    let index_str = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("Missing rule index."))?;
    let index: usize = index_str.parse::<usize>().map_err(|_| {
        anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
    })?;
    if index == 0 {
        bail!("Rule index is 1-based; use 1 for the first rule.");
    }

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;
    let rule = group.rules.get(index - 1).ok_or_else(|| {
        anyhow::anyhow!(
            "No rule at index {index} in [{}] (has {} rule(s)).",
            label,
            group.rules.len()
        )
    })?;

    let rule_id = rule.id;
    drop(ctx);

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::DeleteRule { rule_id });

    println!("Staged: delete rule #{index} from [{}]", label);
    Ok(())
}

/// Stage a value change for an existing rule identified by group label and rule index.
///
/// Expected args: `<group-label> <rule-index> [value]`
///
/// If the value argument is omitted, opens `$EDITOR` pre-filled with the rule's current
/// (or already-staged) value so it can be edited interactively. If the rule's effective
/// comparator is `in`/`not-in`, the value must parse as a JSON array.
pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, rule_id, effective_comparator, effective_value) =
        resolve_rule(args, session)?;

    let raw = match args.get(3) {
        Some(v) => v.to_string(),
        None => open_in_editor(&effective_value)?,
    };
    let new_value = raw.trim().to_string();
    if new_value.is_empty() {
        bail!("No value provided.");
    }
    validate_value_for_comparator(&effective_comparator, &new_value)?;

    println!("Staged: rule #{index} in [{label}] value = {new_value}");

    let mut ctx = session.context.write().unwrap();
    let patch = ctx.get_or_init_segment_patch();
    let op = SegmentPatchOp::SetRuleValue {
        rule_id,
        value: new_value,
    };
    if let Some(existing) = patch
        .ops
        .iter_mut()
        .find(|o| matches!(o, SegmentPatchOp::SetRuleValue { rule_id: rid, .. } if *rid == rule_id))
    {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    Ok(())
}

/// Stage a comparator change for an existing rule identified by group label + 1-based index.
///
/// Expected args: `<group-label> <rule-index> [comparator]`
///
/// If the comparator argument is omitted, opens `$EDITOR` listing every available
/// comparator (the current one marked explicitly) so one can be chosen by leaving it as
/// the only uncommented line. The chosen comparator must match one of the available
/// comparators; if it's `in`/`not-in`, the rule's effective value must parse as a JSON array.
pub fn comparator(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, rule_id, effective_comparator, effective_value) =
        resolve_rule(args, session)?;

    let raw = match args.get(3) {
        Some(c) => c.to_string(),
        None => {
            let content = build_comparator_editor_content(&effective_comparator);
            let edited = open_in_editor(&content)?;
            extract_single_value(&edited)?
        }
    };
    let new_comparator = parse_comparator(raw.trim())?;
    validate_value_for_comparator(&new_comparator, &effective_value)?;

    println!("Staged: rule #{index} in [{label}] comparator = {new_comparator}");

    let mut ctx = session.context.write().unwrap();
    let patch = ctx.get_or_init_segment_patch();
    let op = SegmentPatchOp::SetRuleComparator {
        rule_id,
        comparator: new_comparator,
    };
    if let Some(existing) = patch.ops.iter_mut().find(
        |o| matches!(o, SegmentPatchOp::SetRuleComparator { rule_id: rid, .. } if *rid == rule_id),
    ) {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    Ok(())
}

/// Resolves `<group-label> <rule-index>` from `args` against the current segment context
/// and returns the rule's id together with its effective (committed + staged) comparator
/// and value, so callers can validate/pre-fill without re-reading the context.
fn resolve_rule(
    args: &[Arg],
    session: &Session<Connection>,
) -> anyhow::Result<(String, usize, i32, Comparator, String)> {
    let label = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Missing group label."))?;
    let index_str = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("Missing rule index."))?;
    let index: usize = index_str.parse::<usize>().map_err(|_| {
        anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
    })?;
    if index == 0 {
        bail!("Rule index is 1-based; use 1 for the first rule.");
    }

    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    let rule = group.rules.get(index - 1).ok_or_else(|| {
        anyhow::anyhow!(
            "No rule at index {index} in [{label}] (has {} rule(s)).",
            group.rules.len()
        )
    })?;

    let (comparator, value) = effective_rule_state(rule, ctx.segment_patch.as_ref());
    Ok((label.to_string(), index, rule.id, comparator, value))
}

/// Returns the rule's comparator/value after applying any staged `SetRuleComparator` /
/// `SetRuleValue` ops for it, so edits can be layered on top of previously staged ones.
fn effective_rule_state(rule: &SegmentRule, patch: Option<&SegmentPatch>) -> (Comparator, String) {
    let mut comparator = rule.comparator.clone();
    let mut value = rule.value.clone();

    for op in patch.into_iter().flat_map(|p| &p.ops) {
        match op {
            SegmentPatchOp::SetRuleComparator {
                rule_id,
                comparator: c,
            } if *rule_id == rule.id => {
                comparator = c.clone();
            }
            SegmentPatchOp::SetRuleValue { rule_id, value: v } if *rule_id == rule.id => {
                value = v.clone();
            }
            _ => {}
        }
    }

    (comparator, value)
}

/// Validates that `value` is a JSON array when `comparator` is `in`/`not-in`.
fn validate_value_for_comparator(comparator: &Comparator, value: &str) -> anyhow::Result<()> {
    if matches!(comparator, Comparator::In | Comparator::NotIn)
        && serde_json::from_str::<Vec<serde_json::Value>>(value).is_err()
    {
        bail!(
            "Value must be a valid JSON array for the 'in'/'not-in' comparator, e.g. [\"a\",\"b\"]."
        );
    }
    Ok(())
}

/// Builds editor content listing every available comparator (from `Comparator::iter()`,
/// so a new variant can't be missed here), one per line, with the current one marked
/// explicitly. The user selects a comparator by leaving exactly one line uncommented
/// (mirroring the `SET override` variant picker).
fn build_comparator_editor_content(current: &Comparator) -> String {
    let mut content = String::new();
    content.push_str(
        "# Leave exactly ONE comparator uncommented below to select it.\n\
         # Comment out or delete the rest.\n\n",
    );

    for cmp in Comparator::iter() {
        let marker = if &cmp == current { " (current)" } else { "" };
        content.push_str(&format!("# {cmp}{marker}\n{cmp}\n\n"));
    }

    content
}

fn parse_driver(s: &str) -> anyhow::Result<SegmentDriver> {
    match s {
        "identity" => Ok(SegmentDriver::Identity),
        "environment" => Ok(SegmentDriver::Environment),
        _ if s.starts_with("trait:") => {
            let name = s.trim_start_matches("trait:");
            if name.is_empty() {
                bail!("Trait name cannot be empty. Use: trait:<name>");
            }
            Ok(SegmentDriver::Trait(name.to_string()))
        }
        _ => bail!(
            "Unknown driver '{}'. Expected: identity, environment, trait:<name>",
            s
        ),
    }
}

/// Parses a comparator token (accepting both the canonical hyphenated form and the
/// underscore alias declared on `Comparator`). Driven entirely by `Comparator`'s
/// `strum(serialize = ...)` attributes, so a new variant is automatically accepted here
/// and automatically listed in the error message - nothing to keep in sync by hand.
fn parse_comparator(s: &str) -> anyhow::Result<Comparator> {
    s.parse::<Comparator>().map_err(|_| {
        anyhow::anyhow!(
            "Unknown comparator '{s}'. Expected: {}",
            Comparator::iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}
