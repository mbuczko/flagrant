use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{GroupConnector, Segment, payload::SegmentPatchOp};

use crate::{
    handlers::{internal::effectives as effective, prompt_line},
    printer::{
        menu,
        tabular::{Tabular, segment::format_connector},
    },
};

/// Stage a group addition for the current segment.
///
/// Expected args: `[--and|--and-not] [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let (connector, description) = match args.get(1).map(|a| a.as_ref()) {
        Some("--and") => (
            Some(GroupConnector::And),
            args.get(2).map(|d| d.to_string()),
        ),
        Some("--and-not") => (
            Some(GroupConnector::AndNot),
            args.get(2).map(|d| d.to_string()),
        ),
        other => (None, other.map(str::to_string)),
    };

    let predicted_label = {
        let ctx = session.context.read().unwrap();
        let segment = ctx.segment.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first.")
        })?;
        let staged = ctx
            .segment_patch
            .as_ref()
            .map(|p| p.ops.as_slice())
            .unwrap_or_default();

        predict_next_label(segment, staged)
    };

    let connector_hint = match &connector {
        Some(GroupConnector::And) => " (AND ...)",
        Some(GroupConnector::AndNot) => " (AND NOT ...)",
        None => "",
    };

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::AddGroup {
            connector,
            description,
        });

    println!("Staged: add [{}]{connector_hint}", predicted_label);
    Ok(())
}

/// Print details of a single group, overlaying any staged changes.
///
/// Expected args: `[label]`
///
/// When the label is omitted, opens an interactive menu listing every group in the
/// current segment to choose from instead.
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
            let options = group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups to show. Use `GROUP add` first.");
            }
            menu::select("Show which group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let group = eff
        .groups
        .iter()
        .find(|g| g.label == label)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found."))?;

    group.display(None, &());
    Ok(())
}

/// Stage a group deletion for the current segment.
///
/// Expected args: `[label]` (e.g. "group-1")
///
/// When the label is omitted, opens an interactive menu listing every group in the
/// current segment to choose from instead.
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);

    let label = match args.get(1) {
        Some(label) => label.to_string(),
        None => {
            let options = group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups to delete. Use `GROUP add` first.");
            }
            menu::select("Delete which group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };
    let label: &str = &label;

    // Staged-add groups are appended to `eff.groups` in the same order their `AddGroup` ops
    // appear in `ops`, so the Nth staged-add group found here is always the Nth `AddGroup`
    // op - the same position-based addressing `VariantRef::Staged` uses for staged variants.
    let mut staged_pos = 0usize;
    let mut found = None;

    for g in &eff.groups {
        if g.label == label && !g.is_deleted {
            found = Some(g);
            break;
        }
        if g.is_staged_add {
            staged_pos += 1;
        }
    }

    let group =
        found.ok_or_else(|| anyhow::anyhow!("Group '{label}' not found in current segment."))?;

    if group.is_staged_add {
        let pending = ctx.get_or_init_segment_patch();
        let mut add_count = 0;
        let index = pending.ops.iter().position(|op| {
            matches!(op, SegmentPatchOp::AddGroup { .. }) && {
                let is_target = add_count == staged_pos;
                add_count += 1;
                is_target
            }
        });
        if let Some(i) = index {
            pending.ops.remove(i);
        }
        pending.ops.retain(|op| {
            !matches!(op, SegmentPatchOp::AddRule { group_label, .. } if group_label == label)
                && !matches!(op, SegmentPatchOp::SetGroupDescription { label: l, .. } if l == label)
        });
        println!("Discarded staged group [{label}].");
    } else {
        ctx.get_or_init_segment_patch()
            .ops
            .push(SegmentPatchOp::DeleteGroup {
                label: label.to_string(),
            });
        println!("Staged: delete [{label}]");
    }
    Ok(())
}

/// Stage a group description change.
///
/// Expected args: `[label] [description]`
///
/// When the label is omitted, opens an interactive menu listing every group in the
/// current segment to choose from instead. When the description is omitted, prompts for
/// it inline, pre-filled with the group's current (or already-staged) description;
/// leaving it blank clears the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);

    let label = match args.get(1) {
        Some(label) => label.to_string(),
        None => {
            let options = group_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups to describe. Use `GROUP add` first.");
            }
            menu::select("Describe which group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let label: &str = &label;
    let current: Option<String> = eff
        .groups
        .iter()
        .find(|g| g.label == label && !g.is_deleted)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found in current segment."))?
        .description
        .clone();

    let desc = match args.get(2) {
        Some(d) => Some(d.to_string()),
        None => {
            let Some(edited) = prompt_line("New description", current.as_deref().unwrap_or(""))?
            else {
                println!("Cancelled.");
                return Ok(());
            };
            let new_desc = (!edited.is_empty()).then_some(edited);

            if new_desc == current {
                println!("No changes made.");
                return Ok(());
            }
            new_desc
        }
    };

    println!(
        "Staged: [{label}] description = {}",
        desc.as_deref().unwrap_or_default()
    );

    let op = SegmentPatchOp::SetGroupDescription {
        label: label.to_string(),
        description: desc,
    };
    let patch = ctx.get_or_init_segment_patch();

    if let Some(existing) = patch
        .ops
        .iter_mut()
        .find(|o| matches!(o, SegmentPatchOp::SetGroupDescription { label: l, .. } if l == label))
    {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    Ok(())
}

/// Stage a group connector (joiner) change.
///
/// Expected args: `[label] [and|and-not]`
///
/// When the label is omitted, opens an interactive menu listing every group in the
/// current segment that isn't the first group to choose from instead - the first group
/// has no connector, so it's never offered. When the connector is omitted, opens an
/// interactive menu listing `and`/`and-not` (the current one marked explicitly).
pub fn rejoin(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let segment = ctx
        .segment
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `USE +<segment>` first."))?;
    let patch = ctx.segment_patch.as_ref().filter(|p| !p.is_empty());
    let eff = effective::effective_segment(segment, patch);

    let label = match args.get(1) {
        Some(label) => label.to_string(),
        None => {
            let options = joiner_menu_options(&eff);
            if options.is_empty() {
                bail!("No groups with a joiner to change. The first group has none.");
            }
            menu::select("Change joiner of which group", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No group selected."))?
        }
    };

    let label: &str = &label;
    let current = eff
        .groups
        .iter()
        .find(|g| g.label == label && !g.is_deleted)
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found in current segment."))?
        .connector
        .clone()
        .ok_or_else(|| anyhow::anyhow!("The first group has no joiner to change."))?;

    let connector = match args.get(2) {
        Some(c) => parse_connector(c)?,
        None => {
            let options: Vec<(String, GroupConnector)> =
                [GroupConnector::And, GroupConnector::AndNot]
                    .into_iter()
                    .map(|c| {
                        let label = if c == current {
                            format!("{} (current)", format_connector(&c))
                        } else {
                            format_connector(&c).to_string()
                        };
                        (label, c)
                    })
                    .collect();

            let default = options.iter().position(|(_, c)| *c == current);
            menu::select("Choose a joiner", &options, default)?
                .ok_or_else(|| anyhow::anyhow!("No joiner selected."))?
        }
    };

    println!(
        "Staged: [{label}] joiner = {}",
        format_connector(&connector)
    );

    let op = SegmentPatchOp::SetGroupConnector {
        label: label.to_string(),
        connector,
    };
    let patch = ctx.get_or_init_segment_patch();

    if let Some(existing) = patch
        .ops
        .iter_mut()
        .find(|o| matches!(o, SegmentPatchOp::SetGroupConnector { label: l, .. } if l == label))
    {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    Ok(())
}

/// Parses a joiner token (`and`/`and-not`, also accepting the underscore alias).
fn parse_connector(s: &str) -> anyhow::Result<GroupConnector> {
    match s.to_lowercase().as_str() {
        "and" => Ok(GroupConnector::And),
        "and-not" | "and_not" => Ok(GroupConnector::AndNot),
        _ => bail!("Unknown joiner '{s}'. Expected: and, and-not"),
    }
}

/// Builds the `GROUP joiner` group-picker menu options: one row per non-deleted effective
/// group that isn't the first group (identified by having no connector).
fn joiner_menu_options(eff: &effective::EffectiveSegment) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut labels = Vec::new();

    for g in eff
        .groups
        .iter()
        .filter(|g| !g.is_deleted && g.connector.is_some())
    {
        rows.push((
            format!("[{}]", g.label),
            g.description.clone().unwrap_or_default(),
        ));
        labels.push(g.label.clone());
    }

    menu::align_rows(&rows).into_iter().zip(labels).collect()
}

/// Builds the `GROUP delete` menu options: one row per non-deleted effective group,
/// labeled with its label and description (if any), colon-aligned.
fn group_menu_options(eff: &effective::EffectiveSegment) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut labels = Vec::new();

    for g in eff.groups.iter().filter(|g| !g.is_deleted) {
        rows.push((
            format!("[{}]", g.label),
            g.description.clone().unwrap_or_default(),
        ));
        labels.push(g.label.clone());
    }

    menu::align_rows(&rows).into_iter().zip(labels).collect()
}

/// Predict the label the server will assign to the next new group.
///
/// The server computes `group-{MAX(N)+1}` from groups in the DB at the time of insertion.
/// We simulate that by tracking committed groups and already-staged AddGroup ops.
fn predict_next_label(segment: &Segment, staged_ops: &[SegmentPatchOp]) -> String {
    let mut max_n: u32 = segment
        .groups
        .iter()
        .filter_map(|g| g.label.strip_prefix("group-"))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    for op in staged_ops {
        if let SegmentPatchOp::AddGroup { .. } = op {
            max_n += 1;
        }
    }
    format!("group-{}", max_n + 1)
}
