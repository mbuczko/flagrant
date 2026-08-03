//! REPL command handlers for segment group management.
//!
//! | Command                          | Handler      | Description                                          |
//! |----------------------------------|--------------|------------------------------------------------------|
//! | `GROUP add [--and|--and-not]`    | [`add`]      | Stage a new group on the current segment.            |
//! | `GROUP show <label>`             | [`show`]     | Print details of a group with its rules.             |
//! | `GROUP delete <label>`           | [`delete`]   | Stage a group deletion by label.                     |
//! | `GROUP describe <label> [desc]`  | [`describe`] | Stage a group description change.                    |

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{GroupConnector, Segment, payload::SegmentPatchOp};

use crate::{
    handlers::{internal::effectives as effective, open_in_editor},
    printer::tabular::Tabular,
};

/// Stage a group addition for the current segment.
///
/// Expected args: `[--and|--and-not] [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;
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
        let staged = ctx
            .segment_patch
            .as_ref()
            .map(|p| p.ops.as_slice())
            .unwrap_or_default();

        predict_next_label(&segment, staged)
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
/// Expected args: `<label>`
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let label = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("No group label provided. Use: `GROUP show <label>`."))?;

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

    group.display(None, &());
    Ok(())
}

/// Stage a group deletion for the current segment.
///
/// Expected args: `<label>` (e.g. "group-1")
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;
    let Some(label) = args.get(1) else {
        bail!("No group label provided. Use: `GROUP delete <label>` (e.g. group-1 as the label).");
    };

    if !segment.groups.iter().any(|g| g.label == label.as_ref()) {
        bail!("Group '{label}' not found in current segment.");
    }

    let mut ctx = session.context.write().unwrap();
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::DeleteGroup {
            label: label.to_string(),
        });

    println!("Staged: delete [{}]", label);
    Ok(())
}

/// Stage a group description change.
///
/// Expected args: `<label> [description]`
///
/// If the description is omitted, opens `$EDITOR` pre-filled with the group's current (or
/// already-staged) description so it can be edited interactively; leaving it blank clears
/// the description.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(label) = args.get(1) else {
        bail!("No group label provided. Use: `GROUP describe <label> [description]`.");
    };
    let label: &str = label;

    let mut ctx = session.context.write().unwrap();

    if !ctx
        .segment
        .as_ref()
        .is_some_and(|s| s.groups.iter().any(|g| g.label == label))
    {
        bail!("Group '{label}' not found in current segment.");
    }

    let desc = match args.get(2) {
        Some(d) => Some(d.to_string()),
        None => {
            let current: Option<&str> = ctx
                .segment_patch
                .as_ref()
                .and_then(|p| {
                    p.ops.iter().find_map(|op| match op {
                        SegmentPatchOp::SetGroupDescription {
                            label: l,
                            description,
                        } if l == label => Some(description.as_deref()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| {
                    ctx.segment
                        .as_ref()
                        .unwrap()
                        .groups
                        .iter()
                        .find(|g| g.label == label)
                        .and_then(|g| g.description.as_deref())
                });

            let edited = open_in_editor(current.unwrap_or(""))?;
            let new_desc = (!edited.is_empty()).then_some(edited);

            if new_desc.as_deref() == current {
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

    if let Some(existing) = patch.ops.iter_mut().find(|o| {
        matches!(o, SegmentPatchOp::SetGroupDescription { label: l, .. } if l == label)
    }) {
        *existing = op;
    } else {
        patch.ops.push(op);
    }
    Ok(())
}

fn segment_from_ctx(session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    ctx.segment
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `SEGMENT use <name>` first."))
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
