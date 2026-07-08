//! REPL command handlers for segment group management.
//!
//! | Command                          | Handler      | Description                                          |
//! |----------------------------------|--------------|------------------------------------------------------|
//! | `GROUP add [--and|--and-not]`    | [`add`]      | Stage a new group on the current segment.            |
//! | `GROUP list`                     | [`list`]     | List all groups in the current segment.              |
//! | `GROUP describe <label>`         | [`describe`] | Print details of a group with its rules.             |
//! | `GROUP delete <label>`           | [`delete`]   | Stage a group deletion by label.                     |

use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{GroupConnector, Segment, payload::SegmentPatchOp};

use crate::printer::tabular::Tabular;

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

/// List groups — shows the current committed segment state.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    if let Some(segment) = &ctx.segment {
        let res = ctx.project_resource();
        let fresh = ctx
            .client
            .get::<Segment>(res.subpath(format!("/segments/{}", segment.id)))?;
        fresh.describe(None, &());
        return Ok(());
    }
    bail!("Not in a segment context.");
}

/// Print details of a single group, overlaying any staged changes.
///
/// Expected args: `<label>`
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let label = args.get(1).ok_or_else(|| {
        anyhow::anyhow!("No group label provided. Expected: GROUP describe <label>")
    })?;

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

    group.describe(ctx.segment_patch.as_ref().filter(|p| !p.is_empty()), &());
    Ok(())
}

/// Stage a group deletion for the current segment.
///
/// Expected args: `<label>` (e.g. "group-1")
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;
    let Some(label) = args.get(1) else {
        bail!("No group label provided. Expected: GROUP delete <label> (e.g. group-1)");
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
