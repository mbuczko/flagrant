use anyhow::bail;
use flagrant_client::connection::{Connection, RuleRef};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Comparator, Subject, payload::SegmentPatchOp};
use strum::IntoEnumIterator;

use crate::{
    handlers::internal::{effectives as effective, prompt_line, stage},
    printer::{menu, tabular::Tabular},
};

/// Builds the group-picker menu options shared by [`resolve_rule`] (used by `RULE
/// value`/`comparator`/`delete`) and `RULE show`: one row per non-deleted effective group.
/// Staged-add groups are included - a still-uncommitted group's rules are just as valid a
/// target as a committed group's.
fn effective_group_menu_options(eff: &effective::EffectiveSegment) -> Vec<(String, String)> {
    eff.groups
        .iter()
        .filter(|g| !g.is_deleted)
        .map(|g| (format!("[{}]", g.label), g.label.clone()))
        .collect()
}

/// Builds the rule-picker menu options shared by [`resolve_rule`] and `RULE show`, for a
/// single group: one row per non-deleted effective rule (staged additions included),
/// labeled with its 1-based index, subject, comparator and value.
fn effective_rule_menu_options(rules: &[effective::EffectiveRule]) -> Vec<(String, usize)> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.is_deleted)
        .map(|(i, r)| {
            (
                format!(
                    "rule #{} : {} {} {}",
                    i + 1,
                    r.subject,
                    r.comparator,
                    r.value
                ),
                i + 1,
            )
        })
        .collect()
}

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
/// Expected args: `[group-label] [rule-index]`
///
/// When the group label and/or rule index is omitted, resolves it interactively via
/// [`resolve_rule`]'s two-step menu (group, then rule) - staged additions are included,
/// since a still-uncommitted rule can be deleted just like a committed one.
///
/// For a committed rule, stages a `DeleteRule` op. For a staged addition, there's nothing
/// committed yet, so the pending `AddRule` op is discarded instead.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, target, _, _) = resolve_rule(args, session)?;
    let is_staged = matches!(target, RuleRef::Staged { .. });

    let mut ctx = session.context.write().unwrap();
    stage::discard_rule(ctx.get_or_init_segment_patch(), &target);

    if is_staged {
        println!("Discarded staged rule from [{label}].");
    } else {
        println!("Staged: delete rule #{index} from [{label}]");
    }
    Ok(())
}

/// Print details of a single rule within a group, overlaying any staged changes.
///
/// Expected args: `[group-label] [rule-index]`
///
/// When the group label is omitted, opens an interactive menu listing every group in the
/// current segment (staged additions included, since `show` is read-only) to choose from.
/// When the rule index is omitted, opens an interactive menu listing that group's rules
/// (staged additions included) instead.
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
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
            let options = effective_group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups to show. Use `GROUP add` first.");
            }
            menu::select("Choose a group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let group = eff
        .groups
        .iter()
        .find(|g| g.label == label)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    let index: usize = match args.get(2) {
        Some(index_str) => {
            let index: usize = index_str.parse::<usize>().map_err(|_| {
                anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
            })?;
            if index == 0 {
                bail!("Rule index is 1-based. Use 1 for the first rule.");
            }
            index
        }
        None => {
            let options = effective_rule_menu_options(&group.rules);
            if options.is_empty() {
                bail!("No rules to show in [{label}]. Use `RULE add` first.");
            }
            menu::select(&format!("Choose a rule from [{label}]"), &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No rule selected."))?
        }
    };

    let rule = group
        .rules
        .get(index - 1)
        .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{label}]."))?;

    rule.display(None, &(label, index));
    Ok(())
}

/// Stage a value change for an existing rule identified by group label and rule index.
///
/// Expected args: `[group-label] [rule-index] [value]`
///
/// When the group label and/or rule index is omitted, resolves it interactively via
/// [`resolve_rule`]'s two-step menu (group, then rule) - staged additions are included. If
/// the value argument is omitted, prompts for it inline, pre-filled with the rule's current
/// (or already-staged) value. If the rule's effective comparator is `in`/`not-in`, the
/// value must parse as a JSON array.
pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, target, effective_comparator, effective_value) =
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
    stage::stage_rule_value(ctx.get_or_init_segment_patch(), &target, value);
    Ok(())
}

/// Stage a comparator change for an existing rule identified by group label + 1-based index.
///
/// Expected args: `[group-label] [rule-index] [comparator]`
///
/// When the group label and/or rule index is omitted, resolves it interactively via
/// [`resolve_rule`]'s two-step menu (group, then rule) - staged additions are included. If
/// the comparator argument is omitted, opens an interactive menu listing every available
/// comparator (the current one marked explicitly) to choose from. If it's `in`/`not-in`,
/// the rule's effective value must parse as a JSON array.
pub fn comparator(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (label, index, target, effective_comparator, effective_value) =
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
    stage::stage_rule_comparator(ctx.get_or_init_segment_patch(), &target, comparator);
    Ok(())
}

/// Resolves `[group-label] [rule-index]` from `args` against the current segment context,
/// interactively via a two-step menu (group, then rule) when either is omitted - staged
/// additions are included in both menus, since a still-uncommitted rule is just as valid a
/// target for `RULE value`/`comparator`/`delete` as a committed one (mirrors how `VARIANT`
/// commands address staged variants via `VariantRef`). Returns the rule's label, 1-based
/// index, [`RuleRef`], and effective (committed + staged) comparator/value, so callers can
/// validate/pre-fill without re-reading the context.
fn resolve_rule(
    args: &[Arg],
    session: &Session<Connection>,
) -> anyhow::Result<(String, usize, RuleRef, Comparator, String)> {
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
            let options = effective_group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups with rules. Use `GROUP add` first.");
            }
            menu::select("Choose a group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let eff_group = eff
        .groups
        .iter()
        .find(|g| g.label == label && !g.is_deleted)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    let index: usize = match args.get(2) {
        Some(index_str) => {
            let index: usize = index_str.parse::<usize>().map_err(|_| {
                anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'.")
            })?;

            if index == 0 {
                bail!("Rule index is 1-based; use 1 for the first rule.");
            }
            index
        }
        None => {
            let options = effective_rule_menu_options(&eff_group.rules);
            if options.is_empty() {
                bail!("No rules in [{label}]. Use `RULE add` first.");
            }
            menu::select(&format!("Choose a rule from [{label}]"), &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No rule selected."))?
        }
    };

    let rule = eff_group
        .rules
        .get(index - 1)
        .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{label}]."))?;

    let target = if rule.is_staged_add {
        let position = eff_group.rules[..index - 1]
            .iter()
            .filter(|r| r.is_staged_add)
            .count();
        RuleRef::Staged {
            group_label: label.clone(),
            position,
        }
    } else {
        let rule_id = segment
            .groups
            .iter()
            .find(|g| g.label == label)
            .and_then(|g| g.rules.get(index - 1))
            .ok_or_else(|| anyhow::anyhow!("No rule at index {index} in [{label}]."))?
            .id;
        RuleRef::Committed(rule_id)
    };

    Ok((label, index, target, rule.comparator.clone(), rule.value.clone()))
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
