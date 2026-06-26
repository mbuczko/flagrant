use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Comparator, SegmentDriver, payload::SegmentPatchOp};

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

fn parse_comparator(s: &str) -> anyhow::Result<Comparator> {
    match s {
        "exactly-matches" | "exactly_matches" => Ok(Comparator::ExactlyMatches),
        "does-not-match" | "does_not_match" => Ok(Comparator::DoesNotMatch),
        "contains" => Ok(Comparator::Contains),
        "does-not-contain" | "does_not_contain" => Ok(Comparator::DoesNotContain),
        "greater-than" | "greater_than" => Ok(Comparator::GreaterThan),
        "greater-equal-than" | "greater_equal_than" => Ok(Comparator::GreaterEqualThan),
        "lower-than" | "lower_than" => Ok(Comparator::LowerThan),
        "lower-equal-than" | "lower_equal_than" => Ok(Comparator::LowerEqualThan),
        "in" => Ok(Comparator::In),
        "not-in" | "not_in" => Ok(Comparator::NotIn),
        _ => bail!(
            "Unknown comparator '{}'. Expected: exactly-matches, does-not-match, contains, \
             does-not-contain, greater-than, greater-equal-than, lower-than, lower-equal-than, \
             in, not-in",
            s
        ),
    }
}
