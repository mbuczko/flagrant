use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Comparator, SegmentRule, Subject, payload::SegmentPatchOp};
use strum::IntoEnumIterator;

use crate::{
    handlers::internal::{effectives as effective, prompt_line},
    printer::{menu, tabular::Tabular},
};

/// Stage a rule addition on a group in the current segment.
///
/// Expected args: `<group-label> <subject> <comparator> <value>`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let label = args.get(1).ok_or_else(|| {
        anyhow::anyhow!(
            "Missing group label. Expected: RULE add <group-label> <subject> <comparator> <value>"
        )
    })?;
    let subject = parse_subject(args.get(2).ok_or_else(|| {
        anyhow::anyhow!("Missing subject. Expected: identity, environment, trait:<name>")
    })?)?;
    let comparator = parse_comparator(
        args.get(3)
            .ok_or_else(|| anyhow::anyhow!("Missing comparator."))?,
    )?;
    let value = args
        .get(4)
        .ok_or_else(|| anyhow::anyhow!("Missing value."))?;

    comparator
        .validate_value(value)
        .map_err(|e| anyhow::anyhow!(e))?;

    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);

    if !eff
        .groups
        .iter()
        .any(|g| g.label == label.as_ref() && !g.is_deleted)
    {
        bail!("Group '{label}' not found in current segment. Use `GROUP add` first.");
    }
    drop(ctx);

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::AddRule {
            group_label: label.to_string(),
            subject,
            comparator,
            value: value.to_string(),
        });

    println!("Staged: add rule to [{}]", label);
    Ok(())
}

/// Stage a rule deletion by 1-based index within a group.
///
/// Expected args: `[group-label [rule-index]]`
///
/// When the group label is omitted, opens an interactive menu listing every group in
/// the current segment to choose from. When the rule index is omitted (with or without
/// an explicit group label), opens an interactive menu listing that group's rules
/// instead (rules already staged for deletion are skipped).
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);

    let label = match args.get(1) {
        Some(label) => label.to_string(),
        None => {
            let options = rule_group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups with rules to delete. Use `GROUP add` first.");
            }
            menu::select("Delete a rule from which group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;
    let eff_group = eff
        .groups
        .iter()
        .find(|g| g.label == label)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    let (index, rule_id) = match args.get(2) {
        Some(index_str) => {
            let index: usize = index_str.parse::<usize>().map_err(|_| {
                anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
            })?;

            if index == 0 {
                bail!("Rule index is 1-based; use 1 for the first rule.");
            }

            let rule_id = group
                .rules
                .get(index - 1)
                .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{}].", label))?
                .id;
            (index, rule_id)
        }
        None => {
            let options = rule_delete_menu_options(&group.rules, &eff_group.rules);
            if options.is_empty() {
                bail!("No rules to delete in [{label}]. Use `RULE add` first.");
            }
            menu::select(&format!("Delete which rule from [{label}]"), &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No rule selected."))?
        }
    };

    drop(ctx);

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::DeleteRule { rule_id });

    println!("Staged: delete rule #{index} from [{}]", label);
    Ok(())
}

/// Builds the `RULE delete` group-picker menu options: one row per non-deleted,
/// non-staged-add effective group - a staged-add group has no committed rules to
/// delete yet, so it's excluded.
fn rule_group_menu_options(eff: &effective::EffectiveSegment) -> Vec<(String, String)> {
    eff.groups
        .iter()
        .filter(|g| !g.is_deleted && !g.is_staged_add)
        .map(|g| (format!("[{}]", g.label), g.label.clone()))
        .collect()
}

/// Builds the `RULE delete` rule-picker menu options for a single group: one row per
/// committed rule not already staged for deletion, labeled with its 1-based index,
/// subject, comparator and value. `committed_rules` and `eff_rules` must come from the
/// same group (`effective_segment` preserves committed order before appending staged
/// adds, so they line up by position). Returns `(1-based index, rule id)` pairs so a
/// selection round-trips exactly like typing the index.
fn rule_delete_menu_options(
    committed_rules: &[SegmentRule],
    eff_rules: &[effective::EffectiveRule],
) -> Vec<(String, (usize, i32))> {
    committed_rules
        .iter()
        .zip(eff_rules.iter())
        .enumerate()
        .filter(|(_, (_, er))| !er.is_deleted)
        .map(|(i, (r, _))| {
            (
                format!(
                    "rule #{} : {} {} {}",
                    i + 1,
                    r.subject,
                    r.comparator,
                    r.value
                ),
                (i + 1, r.id),
            )
        })
        .collect()
}

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
        bail!("Rule index is 1-based. Use 1 for the first rule.");
    }

    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);
    let group = eff
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;
    let rule = group
        .rules
        .get(index - 1)
        .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{label}]."))?;

    rule.display(None, &(label.to_string(), index));
    Ok(())
}

/// Stage a value change for an existing rule identified by group label and rule index.
///
/// Expected args: `<group-label> <rule-index> [value]`
///
/// If the value argument is omitted, prompts for it inline, pre-filled with the rule's
/// current (or already-staged) value. If the rule's effective comparator is `in`/`not-in`,
/// the value must parse as a JSON array.
pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, rule_id, effective_comparator, effective_value) =
        resolve_rule(args, session)?;
    let value = match args.get(3) {
        Some(v) => v.to_string(),
        None => match prompt_line("New value", &effective_value)? {
            Some(v) => v,
            None => {
                println!("Cancelled.");
                return Ok(());
            }
        },
    }
    .trim()
    .to_string();

    if value.is_empty() {
        bail!("No value provided.");
    }

    effective_comparator
        .validate_value(&value)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Staged: rule #{index} in [{label}] value = {value}");

    let mut ctx = session.context.write().unwrap();
    let patch = ctx.get_or_init_segment_patch();
    let op = SegmentPatchOp::SetRuleValue { rule_id, value };

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
/// If the comparator argument is omitted, opens an interactive menu listing every
/// available comparator (the current one marked explicitly) to choose from. If it's
/// `in`/`not-in`, the rule's effective value must parse as a JSON array.
pub fn comparator(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, rule_id, effective_comparator, effective_value) =
        resolve_rule(args, session)?;
    let comparator = parse_comparator(
        match args.get(3) {
            Some(c) => c.to_string(),
            None => {
                let options: Vec<(String, Comparator)> = Comparator::iter()
                    .map(|c| {
                        let label = if c == effective_comparator {
                            format!("{c} (current)")
                        } else {
                            c.to_string()
                        };
                        (label, c)
                    })
                    .collect();
                let default = options.iter().position(|(_, c)| *c == effective_comparator);
                menu::select("Choose a comparator", &options, default)?
                    .ok_or_else(|| anyhow::anyhow!("No comparator selected."))?
                    .to_string()
            }
        }
        .trim(),
    )?;

    comparator
        .validate_value(&effective_value)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("Staged: rule #{index} in [{label}] comparator = {comparator}");

    let mut ctx = session.context.write().unwrap();
    let patch = ctx.get_or_init_segment_patch();
    let op = SegmentPatchOp::SetRuleComparator {
        rule_id,
        comparator,
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
    let rule = group
        .rules
        .get(index - 1)
        .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{label}]."))?;

    let (comparator, value) = effective::effective_rule(rule, ctx.segment_patch.as_ref());
    Ok((label.to_string(), index, rule.id, comparator, value))
}

fn parse_subject(s: &str) -> anyhow::Result<Subject> {
    match s {
        "identity" => Ok(Subject::Identity),
        "environment" => Ok(Subject::Environment),
        _ if s.starts_with("trait:") => {
            let name = s.trim_start_matches("trait:");
            if name.is_empty() {
                bail!("Trait name cannot be empty. Use: trait:<name>");
            }
            Ok(Subject::Trait(name.to_string()))
        }
        _ => bail!(
            "Unknown subject '{}'. Expected: identity, environment, trait:<name>",
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
