use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Comparator, Segment, SegmentDriver, SegmentRule,
    payload::NewRulePayload,
};

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

/// Add a rule to a group in the current segment.
///
/// Expected args: `<group-label> <driver> <comparator> <value>`
///
/// Examples:
///   RULE add group-1 identity contains "@bitside.pl"
///   RULE add group-2 trait:version greater-than "3"
///   RULE add group-1 environment exactly-matches "prod"
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `SEGMENT use <name>` first."))?;

    let label = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Missing group label. Expected: RULE add <group-label> <driver> <comparator> <value>"))?;
    let driver_str = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("Missing driver. Expected: identity, environment, trait:<name>"))?;
    let comparator_str = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("Missing comparator."))?;
    let value = args
        .get(4)
        .ok_or_else(|| anyhow::anyhow!("Missing value."))?;

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found in current segment."))?;

    let driver = parse_driver(driver_str)?;
    let comparator = parse_comparator(comparator_str)?;

    let res = ctx.project_resource();
    let rule = ctx.client.post::<_, SegmentRule>(
        res.subpath(format!(
            "/segments/{}/groups/{}/rules",
            segment.id, group.id
        )),
        NewRulePayload {
            driver,
            comparator,
            value: value.to_string(),
        },
    )?;
    drop(ctx);

    println!("Added rule #{} to [{}].", rule.id, label);

    // Refresh segment in context.
    let ctx = session.context.read().unwrap();
    let segment_id = ctx.segment.as_ref().map(|s| s.id).unwrap();
    let res = ctx.project_resource();
    let updated = ctx
        .client
        .get::<Segment>(res.subpath(format!("/segments/{segment_id}")))?;
    drop(ctx);
    session.context.write().unwrap().segment = Some(updated);
    Ok(())
}

/// Delete a rule from a group by 1-based index.
///
/// Expected args: `<group-label> <rule-index>`
///
/// Example: RULE delete group-1 2
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
    let index: usize = index_str
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Rule index must be a positive integer, got '{index_str}'."))?;
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

    let res = ctx.project_resource();
    ctx.client.delete(res.subpath(format!(
        "/segments/{}/groups/{}/rules/{}",
        segment.id, group.id, rule.id
    )))?;
    drop(ctx);

    println!("Deleted rule #{index} from [{}].", label);

    // Refresh segment in context.
    let ctx = session.context.read().unwrap();
    let segment_id = ctx.segment.as_ref().map(|s| s.id).unwrap();
    let res = ctx.project_resource();
    let updated = ctx
        .client
        .get::<Segment>(res.subpath(format!("/segments/{segment_id}")))?;
    drop(ctx);
    session.context.write().unwrap().segment = Some(updated);
    Ok(())
}
